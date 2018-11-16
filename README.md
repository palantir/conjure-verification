# conjure-verification

Behaviour aims to satisfy [RFC 004: Consistent wire-format test cases](https://github.com/palantir/conjure/blob/develop/docs/rfc/004-consistent-wire-format-test-cases.md), but there are a few differences.

This project is split into a [_client verifier_](#client-verifier) and a [_server verifier_](#server-verifier).

## Client verifier
[Client verifier]: #client-verifier

[test-cases.yml](/verification-server-api/test-cases.yml) contains a variety of tests, grouped by type, then endpoint name.
The conjure-defined format for this file is defined [here](./verification-server-api/src/main/conjure/test-cases.conjure.yml).

| Test type | Service definition | Comment |
| --------- | ------------------ | ------- |
| auto-deserialize | [auto-deserialize-service.conjure.yml][] | See [Auto-deserialize tests][] |
| single header | [single-header-service.conjure.yml](./verification-server-api/src/main/conjure/single-header-service.conjure.yml) | Tests the ability to serialize a header param correctly. See [Parameter tests][].
| single query param | [single-query-param-service.conjure.yml](./verification-server-api/src/main/conjure/single-query-param-service.conjure.yml) | Tests the ability to serialize a query param correctly. See [Parameter tests][].
| single path param | [single-path-param-service.conjure.yml](./verification-server-api/src/main/conjure/single-path-param-service.conjure.yml) | Tests the ability to serialize a path param correctly. See [Parameter tests][].

### Workflow

The steps below mostly follow the [RFC 004 workflow](https://github.com/palantir/conjure/blob/develop/docs/rfc/004-consistent-wire-format-test-cases.md#workflow).

First, ensure the necessary artifacts are available in your testing environment:

| Artifact | Maven coordinate | Classifier |
| -------- | ---------------- | ---------- |
| `verification-server.tgz` | `com.palantir.conjure.verification:verification-server` | `osx` or `linux` | 
| `verification-server-test-cases.json` | `com.palantir.conjure.verification:verification-server-test-cases` |
| `verification-server-api.conjure.json` | `com.palantir.conjure.verification:verification-server-api` | 

#### Auto-deserialize tests
[Auto-deserialize tests]: #auto-deserialize-tests
[auto-deserialize-service.conjure.yml]: /verification-server-api/src/main/conjure/auto-deserialize-service.conjure.yml

These tests should verify two things, via the two services defined in [auto-deserialize-service.conjure.yml][]: 
* response bodies are deserialized correctly (via `AutoDeserializeService`)
* previously deserialized conjure values serialized correctly into request bodies (via `AutoDeserializeConfirmService`)

The tests are grouped by `endpoint`, then into positive and negative tests.

The workflow for positive tests is:
1. call the test's endpoint from `AutoDeserializeService`, setting the index to the 0-indexed position of the test.
1. send the received value to the `confirm` endpoint from `AutoDeserializeConfirmService` using the same `EndpointName` and index.

Java example:
```java
Object result = service.receiveDoubleExample(0);
service.confirm(EndpointName.of("receiveDoubleExample"), 0, result);
```

The workflow for negative tests is:
1. call the test's endpoint from `AutoDeserializeService`, setting the index to the (number of positive tests) + the 0-indexed position of the negative test.
1. assert than an exception was thrown because the body could not be deserialized.

Note: Because the tests in each endpoint have the same structure, if the language allows, it's simpler to generate the tests using reflection, rather than hand-rolling a new test for every endpoint.

#### Parameter tests
[Parameter tests]: #parameter-tests

These tests verify that the client can deserialize a value, and is able to send it in a request, as either a path, query or header parameter.
All of these tests are positive, i.e. they should all pass.

The workflow is:
1. deserialize the test from the test cases JSON file.
1. call the test's endpoint from the associated service for that parameter type (see [Client verifier][]) and pass it 

Note: Because the parameter tests in each service & endpoint have the same structure, if the language allows, it's simpler to generate the tests using reflection, rather than hand-rolling a new test for every endpoint.

### Ignoring failing tests

Please see [the Partial Compliance section of RFC 004](https://github.com/palantir/conjure/blob/develop/docs/rfc/004-consistent-wire-format-test-cases.md#partial-compliance).

### Example implementations

* [conjure-java](https://github.com/palantir/conjure-java/tree/2.5.0/conjure-java-client-verifier/src/test/java/com/palantir/conjure/java/compliance)
* [conjure-java-runtime](https://github.com/palantir/conjure-java-runtime/tree/4.7.0/conjure-java-client-verifier/src/test/java/com/palantir/verification)

### docker image

A docker image containing the server along with embedded `test-cases.json` and `verification-server-api.conjure.json` are published to: https://hub.docker.com/r/palantirtechnologies/conjure-verification-server/.

```
$ docker run -p 8000:8000 palantirtechnologies/conjure-verification-server:latest
Listening on http://0.0.0.0:8000

# in another terminal:
$ curl http://localhost:8000/receiveDoubleExample/0
{"value":1.23}
$ curl --data '{"value":1.23}' http://0.0.0.0:8000/confirm/receiveDoubleExample/0 -H 'Content-Type: application/json'
curl --data 'broken' http://0.0.0.0:8000/confirm/receiveDoubleExample/1 -H 'Content-Type: application/json'
```

_Fox maximum logging, add `-e RUST_LOG=debug` to the docker run command._

### Running the server locally

- Ensure you've installed `rustup` as indicated in the [Development](#development) section
- Generate all `test-cases.json` and `verification-server-api.json` files
    ```
    ./gradlew compileTestCasesJson compileIr
    ```
- Start the server on `http://0.0.0.0:8000`
    ```
    cargo run --package conjure-verification-server -- \
        verification-server-api/build/test-cases.json \
        verification-server-api/build/conjure-ir/verification-server-api.conjure.json
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
