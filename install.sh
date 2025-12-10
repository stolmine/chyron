#!/bin/bash
set -e

cargo build --release
cp target/release/chyron ~/.local/bin/
codesign --force --sign - ~/.local/bin/chyron
xattr -cr ~/.local/bin/chyron

echo "Installed chyron to ~/.local/bin/"
