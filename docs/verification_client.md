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
| auto-deserialize | [auto-deserialize-service.yml](/verification-client-api/src/main/conjure/auto-deserialize-service.yml) | `AutoDeserializeService` | See [Server auto-deserialize tests][] |
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

### Example implementations

* [conjure-java](https://github.com/palantir/conjure-java/tree/2.5.0/conjure-java-server-verifier/src/test/java/com/palantir/conjure/java/verification/server)
* [conjure-java-runtime](https://github.com/palantir/conjure-java-runtime/tree/4.7.0/conjure-java-client-verifier/src/test/java/com/palantir/verification)
