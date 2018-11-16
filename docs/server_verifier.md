# Server verifier
[server test-cases.yml]: /verification-client-api/test-cases.yml
[verification-client.conjure.yml]: /verification-client-api/src/main/conjure/verification-client.conjure.yml

The server verifier is a suite of things that 

[server test-cases.yml][] contains a variety of tests, grouped by type, then endpoint name.
The conjure-defined format for this file is defined [here](/verification-client-api/src/main/conjure/test-cases.conjure.yml).

| Test type | Service definition | Comment |
| --------- | ------------------ | ------- |
| auto-deserialize | [auto-deserialize-service.yml](/verification-client-api/src/main/conjure/auto-deserialize-service.yml) | See [Server auto-deserialize tests][] |
| single header | COMING SOON | Tests the ability to deserialize a header param correctly.
| single query param | COMING SOON | Tests the ability to deserialize a query param correctly.
| single path param | COMING SOON | Tests the ability to deserialize a path param correctly.

### Workflow

First, ensure the necessary artifacts are available in your testing environment:

| Artifact | Maven coordinate | Classifier |
| -------- | ---------------- | ---------- |
| `verification-client.tgz` | `com.palantir.conjure.verification:verification-client::${classifier}@tgz` | `osx` or `linux` | 
| `verification-client-test-cases.json` | `com.palantir.conjure.verification:verification-client-test-cases` |
| `verification-client-api.conjure.json` | `com.palantir.conjure.verification:verification-client-api` | 


#### Server auto-deserialize tests
[Server auto-deserialize tests]: #server-auto-deserialize-tests 

The tests are grouped by `endpoint`, then into positive and negative tests.

The workflow for positive tests is:
1. deserialize the test case from the [server test-cases.yml][]
1. call the verification client's `VerificationClientService.runTestCase` endpoint, setting the index to the 0-indexed position of the test.
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
