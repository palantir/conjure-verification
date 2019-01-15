# conjure-verification

Behaviour aims to satisfy [RFC 004: Consistent wire-format test cases](https://github.com/palantir/conjure/blob/develop/docs/rfc/004-consistent-wire-format-test-cases.md), but there are a few differences.

This project has two main components:
* a [_verification server_](/docs/verification_server.md), is a reference server used to test Conjure client generators and libraries.
* a [_verification client_](/docs/verification_client.md), is used to test Conjure server generators and libraries.

## Development

- Install rustup using instructions on https://rustup.rs
- Set up rustup to use the stable toolchain by default (note: nightly's cargofmt output will be different)
    ```
    rustup default stable
    ```
- Create an ssh key if you don't have one, and add it to [github](https://github.com/settings/keys)
- make sure the key is added to the ssh-agent, so that cargo can login to github, in order to access the palantir repository index
    ```
    ssh-add ~/.ssh/id_rsa
    ```
- Install the rust plugin for the IDE of your choice
  - IntelliJ has superior code completion and can get the type of arbitrary expressions (using the Rust plugin), but make sure to tick "Use cargo check to analyze code" - slower, but otherwise IntelliJ won't show most errors inline
  - for VSCode, install [`RLS`](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust) and [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) extension
- To support formatting via rustfmt, [install the component](https://github.com/rust-lang-nursery/rustfmt#installation)
    ```
    rustup component add rustfmt-preview
    ```

## License

This project is made available under the [Apache 2.0 License](/LICENSE).
