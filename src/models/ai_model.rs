use anyhow::{anyhow, bail, ensure, Context, Result};
use candle_core::{quantized::gguf_file, DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::{
    generation::LogitsProcessor,
    models::{llama, quantized_qwen2},
};
use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;
use tracing::{info, warn};

use crate::config::AiConfig;

const FALLBACK_MARKER: &str = "[USE_FALLBACK]";

pub struct Generation {
    pub text: String,
    pub model: String,
    pub fallback_used: bool,
    pub finish_reason: String,
}

pub struct AIModel {
    config: AiConfig,
    tiny: Option<TinyLlama>,
    fallback: Option<QuantizedQwen>,
}

impl AIModel {
    pub fn new(config: AiConfig) -> Self {
        Self {
            config,
            tiny: None,
            fallback: None,
        }
    }

    // Disk loading only. No Hub client or remote inference is used at runtime.
    pub fn load_models(&mut self) -> Result<()> {
        if self.is_ready() {
            return Ok(());
        }
        match TinyLlama::load(&self.config) {
            Ok(model) => {
                self.tiny = Some(model);
                info!("TinyLlama loaded from local safetensors (CPU, F32)");
            }
            Err(error) => warn!("TinyLlama unavailable; requests will use local Qwen: {error:#}"),
        }
        self.fallback = Some(QuantizedQwen::load(&self.config).context(
            "Failed to load local Qwen fallback; copy the GGUF and tokenizer to the VM",
        )?);
        info!("Qwen2.5 Q4_K_M loaded from local GGUF (CPU)");
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.fallback.is_some()
    }

    pub fn generate(
        &mut self,
        prompt: &str,
        system_prompt: &str,
        temperature: f32,
        max_tokens: usize,
        requested_model: Option<&str>,
    ) -> Result<Generation> {
        ensure!(
            self.is_ready(),
            "Local models are not ready; check /api/ready and service logs"
        );
        ensure!(
            temperature.is_finite() && temperature >= 0.0,
            "Invalid temperature"
        );
        ensure!(max_tokens > 0, "max_tokens must be positive");
        let force_fallback = requested_model == Some(self.config.fallback_model_name.as_str());
        if let Some(name) = requested_model {
            ensure!(
                name == "auto" || name == self.config.model_name || force_fallback,
                "Unknown local model: {name}"
            );
        }

        if !force_fallback {
            if let Some(tiny) = &mut self.tiny {
                let budget = max_tokens.min(self.config.tiny_max_tokens);
                match tiny.generate(prompt, system_prompt, temperature, self.config.top_p, budget) {
                    Ok(output) if output.complete && !output.text.trim().is_empty()
                        && !output.text.contains(FALLBACK_MARKER) => {
                        return Ok(Generation {
                            text: output.text,
                            model: self.config.model_name.clone(),
                            fallback_used: false,
                            finish_reason: "stop".to_string(),
                        });
                    }
                    Ok(_) => info!("TinyLlama deferred, returned no answer, or reached its token budget; using local Qwen"),
                    Err(error) => warn!("TinyLlama generation failed; using local Qwen: {error:#}"),
                }
            }
        }

        let fallback = self
            .fallback
            .as_mut()
            .context("Qwen fallback is not loaded")?;
        let output = fallback.generate(
            prompt,
            system_prompt,
            temperature,
            self.config.top_p,
            max_tokens,
        )?;
        ensure!(
            !output.text.trim().is_empty(),
            "Qwen returned an empty response"
        );
        Ok(Generation {
            text: output.text,
            model: self.config.fallback_model_name.clone(),
            fallback_used: true,
            finish_reason: if output.complete { "stop" } else { "length" }.to_string(),
        })
    }
}

struct TinyLlama {
    model: llama::Llama,
    config: llama::Config,
    tokenizer: Tokenizer,
    context_length: usize,
    eos_tokens: Vec<u32>,
}

impl TinyLlama {
    fn load(settings: &AiConfig) -> Result<Self> {
        let root = Path::new(&settings.model_path);
        let config: llama::LlamaConfig = serde_json::from_reader(
            File::open(root.join("config.json")).context("Missing TinyLlama config.json")?,
        )?;
        let mut config = config.into_config(false);
        config.max_position_embeddings =
            config.max_position_embeddings.min(settings.context_length);
        let tokenizer = load_tokenizer(&root.join("tokenizer.json"))?;
        let mut eos_tokens = match &config.eos_token_id {
            Some(llama::LlamaEosToks::Single(id)) => vec![*id],
            Some(llama::LlamaEosToks::Multiple(ids)) => ids.clone(),
            None => Vec::new(),
        };
        if let Some(id) = tokenizer.token_to_id("</s>") {
            eos_tokens.push(id);
        }
        ensure!(!eos_tokens.is_empty(), "TinyLlama has no EOS token");
        let weights = safetensor_files(root)?;
        // Model files must remain unchanged while the memory maps are in use.
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&weights, DType::F32, &Device::Cpu)? };
        let model = llama::Llama::load(vb, &config)?;
        Ok(Self {
            model,
            context_length: config.max_position_embeddings,
            config,
            tokenizer,
            eos_tokens,
        })
    }

    fn generate(
        &mut self,
        message: &str,
        system_prompt: &str,
        temperature: f32,
        top_p: f32,
        max_tokens: usize,
    ) -> Result<Output> {
        let prompt = tiny_chat_prompt(message, system_prompt);
        // A fresh KV cache per request prevents conversations from sharing attention state.
        let mut cache = llama::Cache::new(true, DType::F32, &self.config, &Device::Cpu)?;
        sample(
            &self.tokenizer,
            &prompt,
            self.context_length,
            &self.eos_tokens,
            temperature,
            top_p,
            max_tokens,
            |input, position| self.model.forward(input, position, &mut cache),
        )
    }
}

struct QuantizedQwen {
    model: quantized_qwen2::ModelWeights,
    tokenizer: Tokenizer,
    context_length: usize,
    eos_tokens: Vec<u32>,
}

impl QuantizedQwen {
    fn load(settings: &AiConfig) -> Result<Self> {
        let tokenizer = load_tokenizer(Path::new(&settings.fallback_tokenizer_path))?;
        let mut file = File::open(&settings.fallback_model_path)
            .with_context(|| format!("Missing GGUF: {}", settings.fallback_model_path))?;
        let content = gguf_file::Content::read(&mut file)?;
        ensure!(
            content
                .metadata
                .get("general.architecture")
                .context("Missing GGUF architecture")?
                .to_string()?
                == "qwen2",
            "Expected a Qwen2/Qwen2.5 GGUF"
        );
        if content
            .metadata
            .get("split.count")
            .map(|v| v.to_u16())
            .transpose()?
            .unwrap_or(1)
            > 1
        {
            bail!("Split GGUF files are not supported; provide a single merged Q4_K_M GGUF");
        }
        let context_length = content
            .metadata
            .get("qwen2.context_length")
            .context("Missing Qwen context length")?
            .to_u32()? as usize;
        let eos_tokens = ["<|im_end|>", "<|endoftext|>"]
            .iter()
            .filter_map(|token| tokenizer.token_to_id(token))
            .collect::<Vec<_>>();
        ensure!(!eos_tokens.is_empty(), "Qwen tokenizer has no EOS token");
        let model = quantized_qwen2::ModelWeights::from_gguf(content, &mut file, &Device::Cpu)?;
        Ok(Self {
            model,
            tokenizer,
            context_length: context_length.min(settings.fallback_context_length),
            eos_tokens,
        })
    }

    fn generate(
        &mut self,
        message: &str,
        system_prompt: &str,
        temperature: f32,
        top_p: f32,
        max_tokens: usize,
    ) -> Result<Output> {
        let prompt = qwen_chat_prompt(message, system_prompt);
        // Candle 0.9.1 replaces each layer's KV cache when the first forward uses position 0.
        sample(
            &self.tokenizer,
            &prompt,
            self.context_length,
            &self.eos_tokens,
            temperature,
            top_p,
            max_tokens,
            |input, position| self.model.forward(input, position),
        )
    }
}

fn tiny_chat_prompt(message: &str, system_prompt: &str) -> String {
    // This instruction controls routing only; application behavior comes from the caller.
    let routing = format!("If you cannot confidently complete the task, need deeper reasoning, or need more space, reply only with {FALLBACK_MARKER}.");
    let system = if system_prompt.is_empty() {
        routing
    } else {
        format!("{system_prompt}\n\n{routing}")
    };
    format!("<|system|>\n{system}</s>\n<|user|>\n{message}</s>\n<|assistant|>\n")
}

fn qwen_chat_prompt(message: &str, system_prompt: &str) -> String {
    let system = if system_prompt.is_empty() {
        String::new()
    } else {
        format!("<|im_start|>system\n{system_prompt}<|im_end|>\n")
    };
    format!("{system}<|im_start|>user\n{message}<|im_end|>\n<|im_start|>assistant\n")
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    #[test]
    fn both_models_receive_the_callers_role_and_original_message() {
        let system = "Translate user text into Persian.";
        let message = "Hello, world!";
        assert_eq!(tiny_chat_prompt(message, system), format!(
            "<|system|>\n{system}\n\nIf you cannot confidently complete the task, need deeper reasoning, or need more space, reply only with {FALLBACK_MARKER}.</s>\n<|user|>\n{message}</s>\n<|assistant|>\n"
        ));
        assert_eq!(qwen_chat_prompt(message, system), format!(
            "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{message}<|im_end|>\n<|im_start|>assistant\n"
        ));
        assert!(!qwen_chat_prompt(message, system).contains(FALLBACK_MARKER));
    }

    #[test]
    fn empty_system_prompt_preserves_only_tiny_routing() {
        assert_eq!(
            qwen_chat_prompt("hi", ""),
            "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"
        );
        let tiny = tiny_chat_prompt("hi", "");
        assert!(tiny.contains(FALLBACK_MARKER));
        assert!(!tiny.contains("SelfCare"));
        assert!(!tiny.contains("technical support"));
    }
}

struct Output {
    text: String,
    complete: bool,
}

fn sample(
    tokenizer: &Tokenizer,
    prompt: &str,
    context_length: usize,
    eos_tokens: &[u32],
    temperature: f32,
    top_p: f32,
    max_tokens: usize,
    mut forward: impl FnMut(&Tensor, usize) -> candle_core::Result<Tensor>,
) -> Result<Output> {
    let encoding = tokenizer
        .encode(prompt, true)
        .map_err(|e| anyhow!("Tokenization failed: {e}"))?;
    let prompt_tokens = encoding.get_ids();
    ensure!(!prompt_tokens.is_empty(), "Empty tokenized prompt");
    ensure!(prompt_tokens.len() < context_length,
        "Prompt contains {} tokens; model context limit is {context_length}. Shorten the input or increase the configured context limit.", prompt_tokens.len());
    let budget = max_tokens.min(context_length - prompt_tokens.len());
    let mut sampler =
        LogitsProcessor::new(rand::random(), Some(temperature as f64), Some(top_p as f64));
    let mut generated = Vec::with_capacity(budget);
    let mut input = prompt_tokens.to_vec();
    let mut position = 0;
    let mut complete = false;
    for _ in 0..budget {
        let tensor = Tensor::new(input.as_slice(), &Device::Cpu)?.unsqueeze(0)?;
        let logits = forward(&tensor, position)?
            .squeeze(0)?
            .to_dtype(DType::F32)?;
        let recent_start = generated.len().saturating_sub(64);
        let logits = candle_transformers::utils::apply_repeat_penalty(
            &logits,
            1.1,
            &generated[recent_start..],
        )?;
        let token = sampler.sample(&logits)?;
        if eos_tokens.contains(&token) {
            complete = true;
            break;
        }
        generated.push(token);
        position += input.len();
        input = vec![token];
    }
    let text = tokenizer
        .decode(&generated, true)
        .map_err(|e| anyhow!("Decoding failed: {e}"))?;
    Ok(Output {
        text: text.trim().to_string(),
        complete,
    })
}

fn load_tokenizer(path: &Path) -> Result<Tokenizer> {
    Tokenizer::from_file(path)
        .map_err(|e| anyhow!("Cannot load local tokenizer {}: {e}", path.display()))
}

fn safetensor_files(root: &Path) -> Result<Vec<PathBuf>> {
    let direct = root.join("model.safetensors");
    if direct.is_file() {
        return Ok(vec![direct]);
    }
    let index: serde_json::Value = serde_json::from_reader(
        File::open(root.join("model.safetensors.index.json"))
            .context("Missing TinyLlama safetensors or shard index")?,
    )?;
    let weights = index
        .get("weight_map")
        .and_then(|m| m.as_object())
        .context("Invalid safetensors weight_map")?;
    let files = weights
        .values()
        .map(|v| {
            v.as_str()
                .map(|name| root.join(name))
                .context("Invalid safetensors shard name")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(!files.is_empty(), "No safetensors shards listed");
    for file in &files {
        ensure!(
            file.is_file(),
            "Missing safetensors shard {}",
            file.display()
        );
    }
    Ok(files.into_iter().collect())
}
