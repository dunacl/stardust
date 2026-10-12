# Sourced by the other scripts. Prefers rustup's toolchain (which honours
# rust-toolchain.toml and has the wasm32 target) over any other cargo that
# happens to come first on PATH, e.g. one from MacPorts or Homebrew.
if [ -x "$HOME/.cargo/bin/rustup" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi
