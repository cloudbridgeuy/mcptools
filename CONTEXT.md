# MCPTools

CLI and MCP tools that expose Linear, Jira, Atlas, and related services to coding agents.

## Language

**Issue identifier**:
Linear `KEY-NUMBER` handle for an issue, such as `GUZ-22`.
_Avoid_: ticket ID, issue UUID

**Team key**:
Linear team's short uppercase code. It is the prefix of an **Issue identifier**.
_Avoid_: team name, team id

## Relationships

- An **Issue identifier** contains exactly one **Team key**

## Example dialogue

> **Dev:** "Does `issue update GUZ-22 --state Done` need `--team GUZ`?"
> **Domain expert:** "No. `GUZ-22` is an **Issue identifier**, so the **Team key** is `GUZ`. Pass `--team` only to override that, or when the id is a UUID."

## Behavior

Linear command reference: [Linear](.claude/context/linear.md).

### Requirement: Issue-update state team
Issue update resolves a workflow state name against the **Team key** in the **Issue identifier**. An explicit team argument overrides that key. A state UUID skips team resolution.

#### Scenario: Identifier supplies team
- **WHEN** an issue is updated by **Issue identifier** with a state name and no team argument
- **THEN** the state name resolves against the **Team key** in that identifier

#### Scenario: Explicit team overrides identifier
- **WHEN** an issue is updated by **Issue identifier** with a state name and an explicit team argument
- **THEN** the state name resolves against the explicit team

#### Scenario: Issue UUID with state name
- **WHEN** an issue is updated by UUID with a state name and no team argument
- **THEN** the update fails and asks for a team argument or a state UUID
