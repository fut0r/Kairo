## KairoDB 1.0.1

A small release for the Windows installers. The app and the command line tool work as they did in [1.0.0](https://github.com/fut0r/Kairo/releases/tag/v1.0.0), so there is no need to reinstall if 1.0.0 is working for you.

### Changed

- The Windows `setup.exe` and `.msi` show KairoDB's icon and name on every page, in place of the stock installer pictures.
- `setup.exe` and the uninstaller carry the app icon.
- Windows shows the publisher as `fut0r` in Apps & Features, in the `.msi` and in the app's file properties.

### Not changed

The installers are not code-signed. Windows SmartScreen still reports an unknown publisher, and macOS still asks you to confirm the first launch. The publisher name above does not affect that warning; only a code signature does.

### Install

**Desktop app**: download the installer for your platform below.

**Command line**: download `kairo-windows.exe`, `kairo-macos` or `kairo-linux`, then run `install.ps1` or `install.sh` beside it.

Everything that arrived in 1.0 is in the [1.0.0 release notes](https://github.com/fut0r/Kairo/releases/tag/v1.0.0) and the [changelog](https://github.com/fut0r/Kairo/blob/master/CHANGELOG.md).
