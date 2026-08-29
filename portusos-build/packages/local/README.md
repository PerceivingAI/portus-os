# Local-package staging source

This tracked directory describes the local-package staging boundary. It is not a package output directory.

The build harness stages the Portus payload through `portus-install` into a bounded generated package root. Native Artix package recipes/archives and pacman repository metadata are produced only through the verified Artix packaging path.
