/*
 * (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
 */

package com.palantir.conjure.verification;

import com.google.common.collect.ImmutableMap;
import com.google.common.io.MoreFiles;
import com.google.common.io.RecursiveDeleteOption;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.List;
import java.util.Map;

public final class GenerateServerServices {

    private GenerateServerServices() {}

    public static void main(String... args) throws IOException {
        com.palantir.logsafe.Preconditions.checkArgument(
                args.length == 2, "Usage: <master-test-cases.yml> <gitignored output dir>");
        File file = new File(args[0]);
        File outputDir = new File(args[1]);

        MasterTestCases testCases = TestCasesUtils.parseTestCases(file);

        // Delete old contents
        Files.createDirectories(outputDir.toPath());
        MoreFiles.deleteDirectoryContents(outputDir.toPath(), RecursiveDeleteOption.ALLOW_INSECURE);

        writeServiceDefinition(
                new File(outputDir, "auto-deserialize-service.conjure.yml"),
                "AutoDeserializeService",
                generateAutoDeserializeService(testCases.getBody()));
        writeServiceDefinition(
                new File(outputDir, "auto-deserialize-confirm-service.conjure.yml"),
                "AutoDeserializeConfirmService",
                generateAutoDeserializeConfirmService(testCases.getBody()));
        writeServiceDefinition(
                new File(outputDir, "single-header-service.conjure.yml"),
                "SingleHeaderService",
                generateSingleHeaderService(testCases.getSingleHeaderParam()));
        writeServiceDefinition(
                new File(outputDir, "single-path-param-service.conjure.yml"),
                "SinglePathParamService",
                generateSinglePathParamService(testCases.getSinglePathParam()));
        writeServiceDefinition(
                new File(outputDir, "single-query-param-service.conjure.yml"),
                "SingleQueryParamService",
                generateSingleQueryParamService(testCases.getSingleQueryParam()));
    }

    private static Map<String, Object> generateAutoDeserializeConfirmService(List<BodyTests> body) {
        ImmutableMap.Builder<String, Object> endpoints = ImmutableMap.builder();
        endpoints.put(
                "confirm",
                ImmutableMap.builder()
                        .put("http", "POST /{endpoint}/{index}")
                        .put(
                                "docs",
                                "Send the response received for positive test cases here to verify that it has been "
                                        + "serialized and deserialized properly.")
                        .put(
                                "args",
                                ImmutableMap.builder()
                                        .put("endpoint", "testCases.EndpointName")
                                        .put("index", "integer")
                                        .put("body", "any")
                                        .build())
                        .build());

        body.stream()
                .map(BodyTests::getType)
                .map(TestCasesUtils::parseConjureType)
                .forEach(conjureType -> {
                    String endpointName = TestCasesUtils.typeToEndpointName("receive", conjureType);
                    String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
                    endpoints.put(
                            endpointName,
                            ImmutableMap.of(
                                    "http",
                                    "POST /" + endpointName + "/{index}",
                                    "args",
                                    ImmutableMap.of("index", "integer", "body", typeName)));
                });

        return ImmutableMap.of(
                "name", "Auto Deserialize Confirm Service",
                "package", "com.palantir.conjure.verification.server",
                "default-auth", "none",
                "base-path", "/confirm",
                "endpoints", endpoints.build());
    }

    private static void writeServiceDefinition(File fileName, String serviceName, Map<String, Object> service)
            throws IOException {
        TestCasesUtils.YAML_MAPPER.writeValue(
                fileName,
                createConjureYmlBuilder()
                        .put("services", ImmutableMap.of(serviceName, service))
                        .build());
    }

    private static ImmutableMap.Builder<String, Object> createConjureYmlBuilder() {
        ImmutableMap.Builder<String, Object> builder = ImmutableMap.builder();
        builder.put(
                "types",
                ImmutableMap.of(
                        "conjure-imports",
                        ImmutableMap.of(
                                "examples", "../example-types.conjure.yml",
                                "testCases", "../test-cases.conjure.yml")));
        return builder;
    }

    private static Map<String, Object> generateAutoDeserializeService(List<BodyTests> bodyTests) {
        ImmutableMap.Builder<String, Object> endpoints = ImmutableMap.builder();

        bodyTests.stream()
                .map(BodyTests::getType)
                .map(TestCasesUtils::parseConjureType)
                .forEach(conjureType -> {
                    String endpointName = TestCasesUtils.typeToEndpointName("receive", conjureType);
                    String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
                    endpoints.put(
                            endpointName,
                            ImmutableMap.of(
                                    "http",
                                    "GET /" + endpointName + "/{index}",
                                    "returns",
                                    typeName,
                                    "args",
                                    ImmutableMap.of("index", "integer")));
                });

        return ImmutableMap.of(
                "name", "Auto Deserialize Service",
                "package", "com.palantir.conjure.verification.server",
                "default-auth", "none",
                "base-path", "/body",
                "endpoints", endpoints.build());
    }

    private static Map<String, Object> generateSingleHeaderService(List<SingleHeaderParamTests> testCases) {
        ImmutableMap.Builder<String, Object> endpoints = ImmutableMap.builder();

        testCases.stream()
                .map(SingleHeaderParamTests::getType)
                .map(TestCasesUtils::parseConjureType)
                .forEach(conjureType -> {
                    String endpointName = TestCasesUtils.typeToEndpointName("header", conjureType);
                    String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
                    endpoints.put(
                            endpointName,
                            ImmutableMap.of(
                                    "http",
                                    "POST /" + endpointName + "/{index}",
                                    "args",
                                    ImmutableMap.of(
                                            "index",
                                            "integer",
                                            "header",
                                            ImmutableMap.of(
                                                    "type", typeName,
                                                    "param-type", "header",
                                                    "param-id", "Some-Header"))));
                });

        return ImmutableMap.of(
                "name", "Single Header Service",
                "package", "com.palantir.conjure.verification.server",
                "default-auth", "none",
                "base-path", "/single-header-param",
                "endpoints", endpoints.build());
    }

    private static Map<String, Object> generateSinglePathParamService(List<SinglePathParamTests> testCases) {
        ImmutableMap.Builder<String, Object> endpoints = ImmutableMap.builder();

        testCases.stream()
                .map(SinglePathParamTests::getType)
                .map(TestCasesUtils::parseConjureType)
                .forEach(conjureType -> {
                    String endpointName = TestCasesUtils.typeToEndpointName("pathParam", conjureType);
                    String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
                    endpoints.put(
                            endpointName,
                            ImmutableMap.of(
                                    "http",
                                    "POST /" + endpointName + "/{index}/{param}",
                                    "args",
                                    ImmutableMap.of("index", "integer", "param", typeName)));
                });

        return ImmutableMap.of(
                "name", "Single Path Param Service",
                "package", "com.palantir.conjure.verification.server",
                "default-auth", "none",
                "base-path", "/single-path-param",
                "endpoints", endpoints.build());
    }

    private static Map<String, Object> generateSingleQueryParamService(List<SingleQueryParamTests> testCases) {
        ImmutableMap.Builder<String, Object> endpoints = ImmutableMap.builder();

        testCases.stream()
                .map(SingleQueryParamTests::getType)
                .map(TestCasesUtils::parseConjureType)
                .forEach(conjureType -> {
                    String endpointName = TestCasesUtils.typeToEndpointName("queryParam", conjureType);
                    String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
                    endpoints.put(
                            endpointName,
                            ImmutableMap.of(
                                    "http",
                                    "POST /" + endpointName + "/{index}",
                                    "args",
                                    ImmutableMap.of(
                                            "index",
                                            "integer",
                                            "someQuery",
                                            ImmutableMap.of(
                                                    "type", typeName,
                                                    "param-type", "query",
                                                    "param-id", "foo"))));
                });

        return ImmutableMap.of(
                "name", "Single Query Param Service",
                "package", "com.palantir.conjure.verification.server",
                "default-auth", "none",
                "base-path", "/single-query-param",
                "endpoints", endpoints.build());
    }
}
