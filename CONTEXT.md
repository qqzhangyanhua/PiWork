# CoDo

CoDo is a workspace-based product for directing persistent Agents to complete durable Works. This glossary defines the product boundaries that keep work and memory isolated.

## Work

**Workspace**:
A project root that contains related Works and forms their durable product boundary.
_Avoid_: Team, session

**Work**:
A durable user outcome carried out inside one Workspace, possibly through multiple Assignments and Runs.
_Avoid_: Task, chat, session

**Assignment**:
A unit of responsibility delegated to one Agent as part of a Work.
_Avoid_: Work, Tencent Task

**Run**:
One execution attempt for an Assignment. A retry creates a new Run without replacing the Assignment or Work.
_Avoid_: Work, Assignment, Agent Session

**Work Team**:
The Lead and Members currently assembled for one Work.
_Avoid_: Global Agent catalog, Memory Team

**Agent Definition**:
The reusable role, responsibilities, instructions, and result contract from which Agent Instances are created.
_Avoid_: Agent Instance, Role Template, Prompt

**Agent Instance**:
A persistent member identity that can participate in many Works and retain its own memory.
_Avoid_: Run, session, model

**Agent Session**:
The resumable execution conversation for one Agent Instance inside one Work. It may serve multiple Assignments and may rotate to a new generation.
_Avoid_: Run, Memory Session, Agent Instance

**Capability Pack**:
A reusable professional procedure and contract that can be assigned to an Agent Definition or selected for an Assignment.
_Avoid_: Tool, Extension, Connector

**Result Envelope**:
A Member's structured delivery to the Lead, containing its outcome, evidence, artifacts, validation, uncertainty, and provenance.
_Avoid_: Chat reply, Work Delivery

**Work Delivery**:
The Lead's final synthesized outcome for a Work after required Assignments have reached an acceptable state.
_Avoid_: Member Result Envelope, Run completion

**Work Ledger**:
The reconstructable view of a Work's goal, plan, decisions, Assignments, artifacts, validation, and open questions.
_Avoid_: Source of truth, Agent memory

**Resource**:
User-supplied or Agent-produced content managed by CoDo and linked to a draft, Work, message, or Run.
_Avoid_: Arbitrary workspace file, Result Artifact

**Memory Candidate**:
An Agent-proposed durable memory that has not yet been confirmed or rejected.
_Avoid_: Agent Memory, remote recall result

## Remote Memory

**Memory Team**:
The top-level Tencent memory boundary for one CoDo user or deployment, containing many Workspace Memory Tasks.
_Avoid_: Workspace Team, project Team

**Workspace Memory Task**:
The isolated Tencent memory scope for one Workspace inside the shared Memory Team.
_Avoid_: Work, Assignment, Team

**Memory Session**:
The conversation scope for one Work inside its Workspace Memory Task.
_Avoid_: Run, Workspace

**Memory User**:
The stable local user identity represented in remote memory.
_Avoid_: Agent Instance, Memory Team

## Capability Marketplace

**Capability Catalog**:
The unified collection of discoverable Plugins, MCP Servers, and Skills available to CoDo.
_Avoid_: Installed capabilities, runtime allowlist

**Catalog Package**:
The stable discoverable identity and publisher metadata for one distributable package across all of its releases.
_Avoid_: Package Installation, Package Release, Capability Contribution

**Package Release**:
An immutable published version of a Catalog Package with its manifest, content digest, license, compatibility, and provenance.
_Avoid_: Catalog Package, mutable latest package

**Package Installation**:
A stable, scoped local installation slot for a Catalog Package, with one active Package Release and optional staged or retained releases.
_Avoid_: Catalog Package, Package Release, Capability Binding

**Capability Contribution**:
A Plugin, MCP Server, Skill, or other runtime capability declared by a specific Package Release under a stable contribution key.
_Avoid_: Catalog Package, Package Installation

**Capability Binding**:
A durable configuration that enables and narrows one Capability Contribution for a Workspace, Agent Instance, or Work.
_Avoid_: Capability Grant, Run Capability Snapshot

**Plugin**:
Executable code loaded into an Agent runtime to add tools or runtime behavior.
_Avoid_: Skill, MCP Server, Capability Pack, Pi Package

**MCP Server**:
A local process or remote endpoint that exposes capabilities through the Model Context Protocol.
_Avoid_: Plugin, Connector, Skill

**Skill**:
An on-demand instruction and resource package that teaches an Agent a specialized workflow.
_Avoid_: Plugin, Capability Pack, Prompt Template

**Capability Grant**:
An authorization rule or approval that permits a bounded Capability Operation within a scope.
_Avoid_: Capability Binding, Package Installation, Run Capability Snapshot

**Run Capability Snapshot**:
The immutable, versioned compilation of bindings, grants, policies, and installed release digests that governs one Run.
_Avoid_: Capability Binding, mutable runtime registry

**Runtime Instance**:
A concrete process, connection, index, browser profile, or sandbox launched from an exact Package Release for a bounded scope.
_Avoid_: Package Installation, Run

**Catalog Source**:
An origin from which CoDo discovers capability metadata and releases.
_Avoid_: Publisher, Installation

**Trust Tier**:
CoDo's review classification for a capability release and its publisher provenance.
_Avoid_: Permission grant, popularity rank
