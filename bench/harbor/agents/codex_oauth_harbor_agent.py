"""Harbor installed-agent stub: Codex via ChatGPT/Codex OAuth (no OPENAI_API_KEY).

Wraps `codex exec` (or equivalent headless) using ~/.codex/auth.json on the host,
copied/mounted into the task environment as needed.
"""

from __future__ import annotations


class CodexOauthHarborAgent:
    @staticmethod
    def name() -> str:
        return "codex-oauth"

    def version(self) -> str | None:
        return "0.0.0-stub"

    async def install(self, environment) -> None:
        raise NotImplementedError("Install codex CLI; ensure OAuth auth available to agent user")

    async def run(self, instruction: str, environment, context) -> None:
        raise NotImplementedError("codex exec <instruction>; tee /logs/agent/codex.txt")

    def populate_context_post_run(self, context) -> None:
        raise NotImplementedError("Parse Codex trajectory into AgentContext")
