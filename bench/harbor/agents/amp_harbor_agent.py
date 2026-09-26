"""Harbor installed-agent stub: Amp CLI (`amp -x`) for TB2.1.

Amp is not a Harbor builtin — custom installed agent. Auth via Amp login on cp.local.
"""

from __future__ import annotations


class AmpHarborAgent:
    @staticmethod
    def name() -> str:
        return "amp"

    def version(self) -> str | None:
        return "0.0.0-stub"

    async def install(self, environment) -> None:
        raise NotImplementedError("Install amp CLI; Amp login / token for agent user")

    async def run(self, instruction: str, environment, context) -> None:
        raise NotImplementedError("amp -x <instruction>; tee /logs/agent/amp.txt")

    def populate_context_post_run(self, context) -> None:
        raise NotImplementedError("Parse Amp logs into AgentContext")
