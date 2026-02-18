# Ponderer Documentation

## Architecture Documents

### Living Loop Architecture

The "Living Loop" is a fundamental redesign of Ponderer's core loop to transform it from a reactive polling system into a persistent presence with ongoing concerns, situational awareness, and self-improvement capabilities.

| Document | Purpose |
|----------|---------|
| [LIVING_LOOP_DESIGN.md](./LIVING_LOOP_DESIGN.md) | High-level architecture and philosophy |
| [LIVING_LOOP_SPEC.md](./LIVING_LOOP_SPEC.md) | Technical specification with code structures |
| [LIVING_LOOP_STATUS.md](./LIVING_LOOP_STATUS.md) | Implementation status and transition plan |
| [../scripts/validate_living_loop_acceptance.md](../scripts/validate_living_loop_acceptance.md) | Deterministic living-loop acceptance baseline (standalone backend) |

### Backend Split / API-First

| Document | Purpose |
|----------|---------|
| [BACKEND_API_SPEC.md](./BACKEND_API_SPEC.md) | REST/WS/auth/plugin contract for decoupled frontend/backend |
| [BACKEND_PARITY_VALIDATION.md](./BACKEND_PARITY_VALIDATION.md) | Standalone backend validation matrix and smoke test guidance |

**Key Concepts:**
- **Three-loop architecture:** Ambient (background), Engaged (foreground), Dream (consolidation)
- **Orientation engine:** OODA-style situational synthesis
- **Journal system:** Continuity of inner life
- **Concerns system:** Explicit tracking of ongoing interests
- **ALMA integration:** Self-improving memory architecture

---

## Code Documentation

Each `.rs` file in the source tree has an accompanying `.md` file that documents:
- Purpose of the module
- Component descriptions and interactions
- Contract expectations for dependents
- Implementation notes

See any `src/**/*.md` file for module-specific documentation.

---

## Related Resources

- [ALMA Paper](https://arxiv.org/abs/2602.07755) - Automated meta-Learning of Memory designs
- [Beads Issue Tracker](./.beads/README.md) - AI-native issue tracking in repo
- [AGENTS.md](../AGENTS.md) - Agent instructions for working with this codebase

---

## Contributing

When adding new architecture:
1. Create a design doc in `/docs` with `_DESIGN.md` suffix
2. Create a spec doc with `_SPEC.md` suffix if implementation details are significant
3. Create a status doc with `_STATUS.md` suffix for tracking
4. Create beads tickets from the status doc
5. Update this README with the new document set
