use crate::models::ChatRequest;

#[derive(Debug, Clone, Copy)]
pub enum Complexity {
    Low,
    Medium,
    High,
}

#[derive(Default, Clone)]
pub struct ModelService;

impl ModelService {
    pub fn analyze_complexity(&self, request: &ChatRequest) -> Complexity {
        let length = request.message.chars().count();
        if length < 200 {
            Complexity::Low
        } else if length < 800 {
            Complexity::Medium
        } else {
            Complexity::High
        }
    }
}
