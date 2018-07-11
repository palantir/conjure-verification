# conjure-verification

Behaviour defined in [RFC 004: Consistent wire-format test cases](https://github.com/palantir/conjure/pull/35)

[test-cases.yml](./test-cases.yml) contains a variety of positive and negative tests.  It refers to various Conjure-defined services defined in the API project.

## Running the server

- Ensure you've installed `rustup` as indicated in the [Development](#development) section
- Generate the `test-cases.json` file
    ```
    ./gradlew compileTestCasesJson
    ```
- Start the server on http://127.0.0.1:8000
    ```
    cargo run --package conjure-verification-server -- verification-api/build/test-cases.json
    ```

## Development

- Install rustup via brew
    ```
    brew install rustup
    ```
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
