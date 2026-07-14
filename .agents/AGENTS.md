# Agent Behavior & Verification Rules

These rules govern the agent's behavioral constraints and verification guidelines for this workspace.

---

## 1. Git & Workspace Operations

- **Commit Authorization**: Never perform `git commit` or stage files (`git add`) for final implementation stages without explicit user confirmation and permission.
- **No Silent Reverts/Resets**: If a commit or operation is made prematurely or incorrectly, never run `git reset`, `git checkout`, or other repository-state-altering commands to silently revert it.
  Present the issue to the user, explain the situation, and ask for explicit authorization to execute the cleanup command.
- **Proposals in Docs**: Always save architecture proposals, design drafts, and implementation plans inside the project's version-controlled `docs/` directory rather than the agent's internal
  artifacts folder, so they are visible and readable to the user.

## 2. Infrastructure & Caching

- **Transient Cache Volumes**: Never configure persistent volumes (`volumes:`) or database backup mounts for transient or disposable services (like Valkey/Redis HTML cache containers) unless
  explicitly requested.
- **Strict Version Verification**: Never assume or guess LTS versions, container image tags, or dependency version constraints. Always run search queries or check official registries to verify version
  tags before recommending or writing them.
