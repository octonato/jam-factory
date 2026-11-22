mod app

# List the recipes.
default:
    @just --list --unsorted

# Run the gates, then install Jam Board, both servers and jb.
install: app::install

# Bump the version (patch, minor, major or X.Y.Z), commit and tag it as vX.Y.Z, then install.
release BUMP="patch": (app::release BUMP)
