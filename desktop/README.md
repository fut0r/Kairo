# KairoDB desktop

The desktop app: a Tauri v2 shell (`src-tauri/`) around `kairo-core`, with a React + TypeScript interface (`src/`).

## Run and build

```
npm install
npm run tauri dev                          # live reload
npm run tauri build                        # installers, in ../target/release/bundle
npm run tauri build -- --no-bundle         # just ../target/release/kairo-desktop
```

You need Node.js 22+, Rust 1.94+ and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

## Check

```
npm run lint          # eslint
npm run build         # tsc --noEmit, then the production bundle
npm test              # vitest: unit and component tests
cargo test -p kairo-desktop        # the command layer, from the repository root
```

## How it is put together

```
src/api/types.ts        the wire contract, mirroring the Rust result types
src/api/client.ts       one typed function per Tauri command
src/state/app.tsx       connections, settings, recents, activity
src/components/         DataGrid, CodeEditor, Modal, and the shell
src/views/              Overview, Explorer, Query, Schema, Export, Settings
src/styles/             design tokens and component styles
src-tauri/src/commands.rs   the commands: thin wrappers over kairo-core
src-tauri/src/state.rs      open connections
src-tauri/src/store.rs      settings, recents and history on disk
```

Rules the code keeps to:

- **No database logic in TypeScript.** Parsing, validation, SQL generation and classification all happen in Rust. The syntax colouring in `src/lib/highlight.ts` is cosmetic.
- **Commands return structured data.** Errors are `{ kind, message, detail?, hint? }`; the interface chooses its wording from `kind`.
- **Nothing the interface is given contains a password.**
- **No hard-coded records.** The only fixtures are in `src/test/fixtures.ts`, imported by tests alone.

Dependencies are deliberately few: React, the Tauri API and dialog plugin, and JetBrains Mono. There is no router, state library, component kit or editor library.

## Design

The look follows [kairo.arabdev.site](https://kairo.arabdev.site): the same palette, square corners, 1px hairline grids, Adelle for headings. The tokens are in `src/styles/tokens.css`, which also lists the three places the app differs from the site for contrast.

Layout uses logical properties (`margin-inline`, `border-inline-start`, `text-align: start`), so a right-to-left interface needs only `dir="rtl"` on the root element. Cell text is isolated with `<bdi>`, so Arabic or Hebrew data reads correctly today.

## End-to-end scenarios

`e2e/` drives the built app through the Chrome DevTools Protocol. Windows only, because it relies on WebView2's remote debugging port.

```
npm run tauri build -- --debug --no-bundle
cargo build                                    # from the repository root, for the kairo CLI
npm run e2e                                    # SQLite: 69 checks
npm run e2e -- -Screenshots ..\docs\screenshots
npm run e2e -- -Postgres "postgres://user:secret@localhost:5432/scratch"
```

Each run uses a fresh folder for its database, settings and webview profile, so your own Kairo settings are not touched. `KAIRO_CONFIG_DIR` is what redirects the settings file.

## Icons

`app-icon.svg` is the source. Regenerate the platform icons with:

```
npm run tauri icon app-icon.svg
```

Then delete the generated `src-tauri/icons/android` and `src-tauri/icons/ios` folders; this is a desktop app.
