# SYF Obsidian Vault — Design Specification

**Status:** APPROVED
**Date:** 2026-03-15
**Authors:** Sébastien Bouchard (Fireplank), Claude Code
**Scope:** Knowledge base setup and initial content migration

---

## Section 1 — Purpose

Create a shared Obsidian vault that serves as the central knowledge base for the SYF
ecosystem. The vault is readable and writable by three agents:

- **Sébastien** — via Obsidian GUI (primary author)
- **Claude Code** — via direct filesystem access (organizer, writer)
- **Ana** — via SSH pipeline or future Syncthing sync (reader, inbox writer)

The vault uses plain markdown files. No proprietary format, no database, no API
dependency. Any agent that can read/write files can participate.

---

## Section 2 — Location and Boundaries

**Vault path:** `F:\SYF PROJECT\vault\`

The vault is a **new directory alongside** existing project folders. It does not
encompass the existing repos (AMA, SLIME, WMW, etc.) — those stay in place. Vault
notes **link to** source files using absolute paths when needed.

**What the vault is NOT:**
- Not a copy of the repos (no duplication)
- Not a replacement for git-tracked docs (AMA specs stay in AMA)
- Not an archive organizer (existing .zip/old files are untouched)

---

## Section 3 — Directory Structure

```
F:\SYF PROJECT\vault\
├── projects/
│   ├── ama/
│   │   ├── overview.md
│   │   ├── architecture.md
│   │   ├── threat-model.md
│   │   ├── p0-spec.md
│   │   ├── p1-held.md
│   │   ├── known-issues.md
│   │   └── roadmap.md
│   ├── slime/
│   │   └── overview.md
│   ├── wmw/
│   │   └── overview.md
│   ├── anathema/
│   │   └── overview.md
│   ├── syf-core/
│   │   └── overview.md
│   └── openclaw/
│       └── overview.md
├── research/
│   ├── tools/            ← Frameworks, SDKs, outils découverts
│   ├── bookmarks/        ← Sites, articles, liens sauvegardés
│   ├── videos/           ← Transcriptions, notes de vidéos YouTube
│   └── social/           ← Trouvailles Twitter/X, Reddit, etc.
├── business/
│   ├── cimia.md
│   ├── solutions-ai.md
│   └── capitaine-bob.md
├── infra/
│   ├── machines.md
│   ├── ana.md
│   ├── tailscale.md
│   └── claude-code.md
├── people/
│   └── sebastien.md
├── daily/                ← Daily notes (YYYY-MM-DD.md)
├── brainstorm/           ← Sessions datées (YYYY-MM-DD-topic.md)
├── inbox/                ← Pêle-mêle, à trier plus tard
└── templates/
    ├── project.md
    ├── research.md
    ├── daily.md
    └── decision.md
```

---

## Section 4 — Conventions

### File naming
- `kebab-case.md` — no spaces, no uppercase
- Dates prefix where relevant: `YYYY-MM-DD-topic.md`

### Front matter (YAML)
Every note starts with YAML front matter:

```yaml
---
title: Note Title
date: 2026-03-15
tags: [project, ama, active]
related: "projects/ama/overview"
---
```

### Tags (standardized)
| Tag | Usage |
|-----|-------|
| `#project` | Project notes |
| `#research` | Research/veille techno |
| `#business` | CIMIA, Solutions AI, Bob |
| `#infra` | Machines, networking, tools |
| `#brainstorm` | Brainstorming sessions |
| `#decision` | Architecture/design decisions |
| `#active` | Currently being worked on |
| `#held` | Completed phase, sealed |
| `#idea` | Raw idea, not yet explored |
| `#p0`, `#p1`, `#p2` | Phase tags |

### Internal links
- Use `[[note-name]]` for links within vault
- Use absolute paths for references to files outside vault:
  `Source: F:\SYF PROJECT\AMA\docs\ARCHITECTURE.md`

### Research notes front matter
```yaml
---
title: Tool Name
source: https://github.com/org/repo
found_on: twitter        # youtube, twitter, reddit, web
date: 2026-03-15
tags: [research, tool, agents]
relevant_to: "projects/ama/overview"
status: explored         # bookmarked, explored, integrated
---
```

---

## Section 5 — Multi-Agent Access

### Claude Code (immediate)
- Direct filesystem read/write to `F:\SYF PROJECT\vault\`
- Creates/edits .md files; Obsidian hot-reloads automatically
- No plugin or API needed

### Ana via SSH (immediate)
- Existing pipeline: Ana → SSH/Tailscale → Claude Code → vault filesystem
- Ana can read notes for context, create notes in `inbox/`
- No new infrastructure required

### Syncthing (future — P2)
- Bidirectional sync: Windows Studio ↔ syf-node ↔ S24 FE (mobile)
- Ana gets local filesystem access to `~/vault/` on syf-node
- Obsidian Mobile on Android reads synced vault
- Not needed for initial setup — SSH pipeline works now

### Write permissions
| Agent | Create | Modify | Restricted |
|-------|--------|--------|------------|
| Sébastien | Anywhere | Anywhere | — |
| Claude Code | Anywhere | Anywhere except personal daily entries | — |
| Ana | `inbox/`, `daily/` | Notes she created | No project/spec edits without request |

---

## Section 6 — Initial Content (~25 notes)

### Projects (7 notes)
Migrated from MEMORY.md and AMA docs. The AMA directory tree (Section 3) shows
7 files; only `overview.md` and `roadmap.md` are seeded now. The remaining 5
(`architecture.md`, `threat-model.md`, `p0-spec.md`, `p1-held.md`, `known-issues.md`)
are created as the project evolves.

- `projects/ama/overview.md` — Status, definition, stack, test count
- `projects/ama/roadmap.md` — P2 priorities from KNOWN_ISSUES + Qwen review
- `projects/slime/overview.md` — Execution membrane
- `projects/wmw/overview.md` — World Machine Web, 3 layers, C
- `projects/anathema/overview.md` — Cyborg project
- `projects/syf-core/overview.md` — Mathematical invariant law, triad
- `projects/openclaw/overview.md` — Ana, Telegram, pipeline

### Research (7 notes)
From recent conversations:
- `research/tools/copilotkit.md` — React agent SDK, not relevant now
- `research/tools/bmad-method.md` — IDE agent personas
- `research/tools/langchain-deep-agents.md` — Deep agent patterns
- `research/tools/lossless-claw.md` — Martian engineering tool
- `research/tools/buzz-whisper.md` — Local STT, Whisper GUI
- `research/bookmarks/marktechpost.md` — AI news site
- `research/bookmarks/ai-tutorial-codes.md` — 112+ tutorial repo

### Business (3 notes)
- `business/cimia.md` — Cours IA, 4 villes, Skool
- `business/solutions-ai.md` — B2B consulting SLSJ
- `business/capitaine-bob.md` — Restaurant, transition

### Infra (4 notes)
- `infra/machines.md` — 6 machines inventory table
- `infra/ana.md` — Soul, identity, pipeline, skills
- `infra/tailscale.md` — VPN mesh, IPs, SSH config
- `infra/claude-code.md` — Config, plugins, bypass settings

### Other (4 notes)
- `people/sebastien.md` — Profile, interests, context
- `daily/2026-03-15.md` — Today's work log
- `brainstorm/2026-03-15-obsidian-vault.md` — This design session
- `templates/` — 4 template files (project, research, daily, decision)

### What is NOT migrated
- Full AMA specs (stay in `F:\SYF PROJECT\AMA\docs\`)
- Archive files (.zip, old whitepapers, logos)
- GPT conversation exports

Notes **link to** source files rather than duplicating content.

---

## Section 7 — Non-Goals

- No Syncthing setup in this phase
- No archive cleanup of `F:\SYF PROJECT\`
- No Obsidian plugin configuration (user handles GUI preferences)
- No git tracking of the vault (Obsidian handles its own state)
- No migration of GPT/Qwen conversation archives

---

## Appendix — Template Skeletons

### project.md
```yaml
---
title: Project Name
repo: https://github.com/AnathemaOfficial/repo
status: active         # active, held, planned, archived
phase: p0              # p0, p1, p2
language: rust
tags: [project]
date: {{date}}
---
```
## Overview
## Status
## Architecture
## Links

### research.md
```yaml
---
title: Tool/Article Name
source: https://example.com
found_on: web          # youtube, twitter, reddit, web
date: {{date}}
tags: [research]
relevant_to: ""
status: bookmarked     # bookmarked, explored, integrated
---
```
## What is it?
## Relevance to SYF
## Key takeaways

### daily.md
```yaml
---
date: {{date}}
tags: [daily]
---
```
## Done today
## Notes
## Tomorrow

### decision.md
```yaml
---
title: Decision Title
date: {{date}}
tags: [decision]
related: ""
status: decided        # proposed, decided, superseded
---
```
## Context
## Options considered
## Decision
## Rationale
