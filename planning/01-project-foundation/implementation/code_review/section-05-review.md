# Code Review: Section 05 - Tauri Desktop App Scaffold

Overall the implementation is solid and covers the majority of the plan's requirements. The file structure, tests, configuration, tray setup, and database module all match the specification. However, there are several issues worth flagging, ranging from a potential runtime defect to minor deviations.

**HIGH SEVERITY - Potential Ownership Issue with tauri_specta::Builder**

In `lib.rs` lines 52-69, the `builder` variable is used in three places: (1) `.export()` on line 58, (2) `.invoke_handler()` on line 67, and (3) `builder.mount_events(app)` on line 69. If `export()` consumes `self` (takes ownership), this code will fail to compile in debug builds. The `#[cfg(debug_assertions)]` attribute only applies to the immediately following statement, so in release builds the export is skipped and the builder survives. This is a conditional compilation landmine that may only surface in debug builds.

**MEDIUM SEVERITY - Specta Integration Pattern Deviates from Plan**

The plan specifies `.plugin(builder.into_plugin())` for registering the IPC router. The implementation instead uses `.invoke_handler(builder.invoke_handler())` and `builder.mount_events(app)`. This was an intentional change based on the actual tauri-specta v2 RC API (the `.into_plugin()` method is not available in the RC version used).

**MEDIUM SEVERITY - specta-typescript Version Mismatch**

The plan specifies `specta-typescript = 0.0.7`. The implementation uses `0.0.9`. This is a reasonable upgrade as 0.0.7 is not available and 0.0.9 is the current latest.

**LOW SEVERITY - Missing Prettier Formatter for TypeScript Export**

The plan specifies `.formatter(specta_typescript::formatter::prettier)` on the Typescript export builder. The implementation omits this. The generated `bindings.ts` file will have unformatted output.

**LOW SEVERITY - Silent Error Swallowing in Tray Event Handler**

The `window.hide()`, `window.show()`, and `window.set_focus()` calls discard errors with `let _ = ...`. Should at minimum log a warning.

**LOW SEVERITY - db and commands Modules are pub**

Both `db` and `commands` are declared as `pub mod`. The plan shows them as private `mod`. Making these public exposes internal implementation details.

**INFORMATIONAL - Pinned RC Versions**

Exact RC version pins (`=2.0.0-rc.22` for specta, `=2.0.0-rc.21` for tauri-specta) are a good practice for RC dependencies.
