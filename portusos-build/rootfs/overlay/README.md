# Rootfs overlay source

This directory is the tracked source root for static PortusOS rootfs overlay files.

Keep it intentionally small. Files already owned by `runtime/install/install.toml` are staged through `portus-install` and must not be copied here. Configuration derived from machine-readable build contracts is generated into `portusos-build/work/` during a build plan/render step rather than committed as unexplained output.

The exact Artix `artools` overlay placement is owned by the verified Artix adapter and ISO profile contracts.
