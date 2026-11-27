# HarmonyOS NEXT PC client

- `designed` The PC ability hosts a native ArkUI XComponent. The first frame shows the current seat's seeded globe, including visible terrain and pieces. Surface resizing uses the actual framebuffer dimensions. The game session survives replacement of its native surface.
- `designed` The End turn control dispatches through the same client session as the other local clients. Graphics and window callbacks cannot mutate simulation state directly.
- `designed` On a HarmonyOS NEXT PC, the client must select a Vulkan adapter compatible with the XComponent native window, submit the game mesh and present to that window. Acceptance requires a signed installed HAP, an identified PC, current-process logs with the adapter and frame dimensions, and a screenshot visibly showing the game. Host builds, emulators and prior upstream tests have separate results.
