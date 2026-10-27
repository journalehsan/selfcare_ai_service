#!/usr/bin/env python3
"""Configuration preservation and rollback checks; no Docker daemon required."""
import copy
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "service_update", Path(__file__).resolve().parents[2] / "deploy/update-service.py")
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)


def existing():
    identifier = "a" * 64
    return {
        "Id": identifier,
        "Config": {"Image": "old-image", "Hostname": identifier[:12],
                   "Env": ["SYSTEM_PROMPT=First line\nSecond line", "MAX_TOKENS=32"],
                   "Labels": {"com.selfcare.offline": "true"},
                   "User": "10001:10001", "Entrypoint": ["/usr/local/bin/selfcare_ai_service"]},
        "HostConfig": {"AutoRemove": False, "ReadonlyRootfs": True,
                       "PortBindings": {"5732/tcp": [{"HostIp": "127.0.0.1", "HostPort": "15732"}]},
                       "Mounts": [{"Type": "bind", "Source": "/original/models", "Target": "/models", "ReadOnly": True}],
                       "RestartPolicy": {"Name": "unless-stopped"},
                       "CapDrop": ["ALL"], "SecurityOpt": ["no-new-privileges"],
                       "Tmpfs": {"/tmp": "rw,noexec,nosuid,size=64m"}},
        "Mounts": [{"Type": "volume", "Name": "original-data", "Destination": "/app/data", "RW": True}],
        "State": {"Running": True, "Paused": False},
        "NetworkSettings": {"Networks": {"original-network": {
            "Aliases": ["selfcare-ai", "custom-alias", identifier[:12]],
            "IPAddress": "172.18.0.7", "EndpointID": "generated-endpoint"}}},
    }


class ServiceUpdateTests(unittest.TestCase):
    def test_preserves_settings_and_volume_data_without_pinning_dynamic_addresses(self):
        previous = existing()
        original = copy.deepcopy(previous)
        result = update.replacement_spec(previous, "new-image")
        self.assertEqual(result["Image"], "new-image")
        self.assertEqual(result["Env"], previous["Config"]["Env"])
        for key, value in previous["HostConfig"].items():
            if key != "Mounts":
                self.assertEqual(result["HostConfig"][key], value)
        self.assertIn({"Type": "volume", "Source": "original-data", "Target": "/app/data", "ReadOnly": False}, result["HostConfig"]["Mounts"])
        self.assertEqual(result["NetworkingConfig"]["EndpointsConfig"], {
            "original-network": {"Aliases": ["selfcare-ai", "custom-alias"]}})
        self.assertEqual(previous, original)

    def test_rejects_auto_removed_containers_before_stopping_them(self):
        previous = existing()
        previous["HostConfig"]["AutoRemove"] = True
        with self.assertRaises(RuntimeError):
            update.replacement_spec(previous, "new-image")

    def test_does_not_duplicate_explicit_volume_mounts(self):
        previous = existing()
        previous["HostConfig"]["Mounts"].append({"Type": "volume", "Source": "original-data", "Target": "/app/data"})
        result = update.replacement_spec(previous, "new-image")
        self.assertEqual(len(result["HostConfig"]["Mounts"]), 2)

    def test_success_retains_previous_container_and_does_not_touch_redis(self):
        with patch.object(update, "inspect", return_value=existing()), \
                patch.object(update, "create_container", return_value="candidate"), \
                patch.object(update, "docker") as docker, \
                patch.object(update, "wait_ready") as ready, \
                patch.object(update, "reload_proxy") as reload:
            update.update_service("new-image")
            ready.assert_called_once_with("selfcare-ai", 300)
            reload.assert_called_once_with("selfcare-ai")
            self.assertEqual(docker.call_args_list[0].args, ("stop", "--time", "30", "selfcare-ai"))
            self.assertFalse(any("redis" in str(call) or "rm" in call.args for call in docker.call_args_list))

    def test_readiness_failure_restores_old_container(self):
        with patch.object(update, "inspect", return_value=existing()), \
                patch.object(update, "create_container", return_value="candidate"), \
                patch.object(update, "docker") as docker, \
                patch.object(update, "wait_ready", side_effect=[RuntimeError("not ready"), None]) as ready, \
                patch.object(update, "reload_proxy") as reload:
            with self.assertRaisesRegex(RuntimeError, "not ready"):
                update.update_service("new-image")
            docker.assert_any_call("rm", "--force", "candidate", check=False)
            docker.assert_any_call("start", "selfcare-ai")
            self.assertEqual(ready.call_count, 2)
            reload.assert_called_once_with("selfcare-ai")

    def test_unrelated_container_is_never_replaced(self):
        previous = existing()
        previous["Config"]["Labels"] = {}
        with patch.object(update, "inspect", return_value=previous), \
                patch.object(update, "create_container") as create:
            with self.assertRaisesRegex(RuntimeError, "unrelated"):
                update.update_service("new-image")
            create.assert_not_called()


if __name__ == "__main__":
    unittest.main()
