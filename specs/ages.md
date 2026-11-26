# Late-game ages

- `designed` Each seat advances independently through the ordered ages in its ruleset. The first age is available at the start. A later age requires all listed technologies and civics as well as the preceding age. A ruleset may omit ages.
- `designed` Resolving research can cross several consecutive thresholds. Evaluate them immediately after a completion, including a research order paid for from stored points. When both tracks resolve at the same round boundary, apply the transition hooks after their ordinary resolution.
- `designed` A transition retains cities, units, completed nodes, selected projects and active policies. It carries both banks of research points by default. An age may clear the science bank, the culture bank, or both on entry. Each reset applies once, even when saving or reconnecting at the boundary.
- `designed` Capabilities accumulate through reached ages. A policy may require an age in addition to a civic. A seat can discover and adopt eligible policies through its observation and the normal policy command. Other seats' ages and unlocks are private.
- `designed` The base game starts in the Founding age. Technology `tech_12` opens the Industrial age and `tech_20` opens the Orbital age after Industrial. Orbital grants `orbital_access` and permits the orbital research policy. The two base transitions retain stored research.

The acceptance game uses two seats. It checks a policy order before and after the unlock, the other seat's observation, and the same crossing through a restored save and command replay. [Progression](../docs/adr/0008-progression-and-victories.md) · [Transition rules](../docs/adr/0026-late-game-ages.md)
