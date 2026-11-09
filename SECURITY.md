# Security

Natural Skin is a native desktop application. It reads the image selected by the user and writes a new result file. It is no longer a WASM module running inside PhotoCraft's sandbox. The desktop workflow does not require network access or a connection to the editor.

The optional development control client accepts authenticated loopback endpoints and explicit token files only. Integration probes launch their own PhotoCraft config and documents; never test against a user's ordinary editor session or log credentials. Do not automatically retry a timed-out editor mutation.

The pcraft adapter validates format/layout, ZIP structure, CRC and content hashes, and imposes decode/file budgets. It accepts a limited single-raster subset. It never extracts archive paths to the filesystem. Output publication refuses existing paths; a revision guard is not atomic host application.

The normal action workflow uses a per-user exchange directory and private immutable snapshots. It starts no network listener and does not enable PhotoCraft Agent Control. Application paths come from explicit desktop settings, never image metadata. The generated installer contains only action-recording, layer-duplication, save-copy and temporary-document close commands. The user imports it through PhotoCraft's normal Scripts menu. Completed images and snapshots stay on disk until the user removes them.

Login startup is opt-in. It creates only a Natural Skin LaunchAgent on macOS or a named value in the current user's Windows Run key; disabling it removes that registration. No system-wide service or administrator permission is required. Move the app to its permanent location before enabling startup.

For suspected memory-safety, file-integrity or control-bridge issues, use [GitHub private vulnerability reporting](https://github.com/wnanbei/photocraft-natural-skin/security/advisories/new) if enabled. Otherwise ask for a private reporting channel without posting private images or exploit details. Include native/PhotoCraft versions, OS, a synthetic reproduction and artifact SHA-256; never attach credentials or private portraits.

Native 0.2.0 is a development build, not a published supported release line. Historical WASM tags describe the old product only.
