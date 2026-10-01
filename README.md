<p align="right">
<a href="https://autorelease.general.dmz.palantir.tech/palantir/conjure-verification"><img src="https://img.shields.io/badge/Perform%20an-Autorelease-success.svg" alt="Autorelease"></a>
</p>

# conjure-verification

Behaviour aims to satisfy [RFC 004: Consistent wire-format test cases](https://github.com/palantir/conjure/blob/master/docs/rfc/004-consistent-wire-format-test-cases.md), but there are a few differences.

This project has two main components:
* a [_verification server_](/docs/verification_server.md), is a reference server used to test Conjure client generators and libraries.
* a [_verification client_](/docs/verification_client.md), is used to test Conjure server generators and libraries.

## Development

The gradle build (java) and cargo build (rust) are indepndent of one another. Gradle is responsible
for generating the Conjure definitions and Java test cases, and cargo is responsible for running the
rust tests. Gradle needs a minimum of Java 17 to build.

- Run `./gradlew build` to generate the Conjure definitions and Java test cases.
- Install rustup using instructions on https://rustup.rs . Choose any modern version of rust when
    prompted.
- The Rust toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml); rustup will automatically download and use it (including the `rustfmt` and `clippy` components) for any cargo command run in this repo. Note that you must be using a rustup supplied version of `cargo` to use this bootstrapping feature. If `cargo` was installed using something like brew, or is very old, then this bootstrapping will be ignored.
- Build the Rust workspace and run the test suite:
    ```
    cargo build --workspace
    cargo test --workspace
    ```
  If any errors occur, try running `./gradlew build` first — the server tests read the generated test cases from `verification-server-api/build`.

  A few `conjure-verification-http-client` tests need external setup: `google` requires internet access, and `google_http_proxy`, `google_https_proxy`, and `assume_http2_tls` are `#[ignore]`d because they require a local TLS proxy (run them with `cargo test -- --ignored`).
- If inspecting/editing code, install the rust plugin for the IDE of your choice.
  - IntelliJ has superior code completion and can get the type of arbitrary expressions (using the Rust plugin), but make sure to tick "Use cargo check to analyze code" - slower, but otherwise IntelliJ won't show most errors inline
  - for VSCode, install the [`rust-analyzer`](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension and [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) for debuggingA
- For vibecoding, direct the agent to read this README in lieu of an AGENTS.md file.

## License

This project is made available under the [Apache 2.0 License](/LICENSE).
