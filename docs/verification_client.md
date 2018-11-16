# Verification client
[test-cases.yml]: /verification-client-api/test-cases.yml
[verification-client.conjure.yml]: /verification-client-api/src/main/conjure/verification-client.conjure.yml

The _verification client_ is a server that can be used to run test cases against a server-under-test using a reference client implementation.

[test-cases.yml][] contains a variety of tests, grouped by type, then endpoint name.
The conjure-defined format for this file is defined [here](/verification-client-api/src/main/conjure/test-cases.conjure.yml).

### Prerequisites

First, ensure the necessary artifacts are available in your testing environment:

| Artifact | Maven coordinate | Classifier |
| -------- | ---------------- | ---------- |
| `verification-client.tgz` | `com.palantir.conjure.verification:verification-client::${classifier}@tgz` | `osx` or `linux` | 
| `verification-client-test-cases.json` | `com.palantir.conjure.verification:verification-client-test-cases` |
| `verification-client-api.conjure.json` | `com.palantir.conjure.verification:verification-client-api` | 

The [server under test][] also needs to be implemented and made available to the test harness.

### Workflow

#### Server under test
[server under test]: #server-under-test

Using the generator and runtime that are being tested:
1. generate server bindings for services found in the verification-client-api IR (`verification-client-api.conjure.json`).
1. Implement all the test services mentioned in the table under [workflow](#workflow). Every endpoint should have exactly one argument and one return type. The implementations should just return the argument.

#### Test harness

The test harness should ensure that both the [server under test][] and the verification client are up and running 
before running the tests, then stop them after it's done running the tests.

To run the verification client, extract the executable out of the `verification-client.tgz` and run it. There should only be one file inside the archive.

For each test found in the [test-cases.yml][] file, the harness should invoke the `VerificationClientService`'s `runTestCase` endpoint, passing the endpoint name, test index (0-indexed) and URL to the _server under test_.
Note: For negative [Auto-deserialize tests][], the index should be set to (number of positive tests) + the 0-indexed position of the negative test.

### Types of test cases

| Test type | Service definition | Service to implement | Comment |
| --------- | ------------------ | -------------------- | ------- |
| auto-deserialize | [auto-deserialize-service.yml](/verification-client-api/src/main/conjure/auto-deserialize-service.yml) | `AutoDeserializeService` | See [Auto-deserialize tests][] |
| single header | not implemented yet | | Tests the ability to deserialize a header param correctly.
| single query param | not implemented yet | | Tests the ability to deserialize a query param correctly.
| single path param | not implemented yet | | Tests the ability to deserialize a path param correctly.

#### Auto-deserialize tests
[Auto-deserialize tests]: #auto-deserialize-tests

The tests are grouped by `endpoint`, then into positive and negative tests.

Java example:
```java
Object result = service.receiveDoubleExample(0);
service.confirm(EndpointName.of("receiveDoubleExample"), 0, result);
```

The test harness doesn't need to assert that negative test cases failed. The `VerificationClientService` encapsulates
all of that logic, and will return an error if a test didn't behave as expected. 

Note: Because the tests in each endpoint have the same structure, if the language allows, it's simpler to generate the tests using reflection, rather than hand-rolling a new test for every endpoint.

### Ignoring failing tests

Please see [the Partial Compliance section of RFC 004](https://github.com/palantir/conjure/blob/develop/docs/rfc/004-consistent-wire-format-test-cases.md#partial-compliance).

### Example implementations

* [conjure-java](https://github.com/palantir/conjure-java/tree/2.5.0/conjure-java-server-verifier/src/test/java/com/palantir/conjure/java/verification/server)
* [conjure-java-runtime](https://github.com/palantir/conjure-java-runtime/tree/4.7.0/conjure-java-client-verifier/src/test/java/com/palantir/verification)

### docker image

A docker image containing the server along with embedded `test-cases.json` and `verification-client-api.conjure.json` are published to: https://hub.docker.com/r/palantirtechnologies/conjure-verification-client/.

```bash
$ docker run -p 8000:8000 palantirtechnologies/conjure-verification-client:latest
Listening on http://0.0.0.0:8000

# in another terminal:
# start the server-under-test
$ nc -l 1234

# in another terminal:
$ curl http://localhost:8000/runTestCase -H 'Content-Type: application/json' --data '{"endpointName": "getDoubleExample", "testCase": 0, "baseUrl": "http://127.0.0.1:1234"}'
```

_Fox maximum logging, add `-e RUST_LOG=debug` to the docker run command._

### Running the verification-client server locally

- Ensure you've installed `rustup` as indicated in the [Development](/README.md#development) section
- Generate all `test-cases.json` and `verification-client-api.json` files
    ```bash
    ./gradlew compileTestCasesJson compileIr
    ```
- Start the server on `http://0.0.0.0:8000`
    ```bash
    cargo run --package conjure-verification-client -- \
        verification-client-api/build/test-cases.json \
        verification-client-api/build/conjure-ir/verification-client-api.conjure.json
    ```
