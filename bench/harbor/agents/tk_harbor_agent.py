"""Harbor BaseInstalledAgent stub for telekinesis `tk exec` (TB 2.1).

Flesh out against the installed harbor package once Docker + `pip install harbor`
are available. Do not import harbor at module import time in CI without harbor.
"""

from __future__ import annotations

# Pseudocode / interface sketch — real imports after `pip install harbor`:
# from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
# from harbor.environments.base import BaseEnvironment
# from harbor.models.agent.context import AgentContext

class TkHarborAgent:  # subclass BaseInstalledAgent when harbor is installed
    """Installed agent: install `tk`, run `tk exec` headless on the task instruction."""

    @staticmethod
    def name() -> str:
        return "tk"

    def version(self) -> str | None:
        return "0.0.0-stub"

    async def install(self, environment) -> None:
        """Install tk binary + write model/provider config for the fixed bench model."""
        raise NotImplementedError("Phase C: install tk via install.sh or mounted binary")

    async def run(self, instruction: str, environment, context) -> None:
        """tk exec --json --cwd <workdir> --model <fixed> <instruction>; tee /logs/agent/tk.txt"""
        raise NotImplementedError("Phase C: headless tk exec")

    def populate_context_post_run(self, context) -> None:
        """Parse /logs/agent/tk.txt + optional JSON summary into AgentContext."""
        raise NotImplementedError("Phase C: populate context")
