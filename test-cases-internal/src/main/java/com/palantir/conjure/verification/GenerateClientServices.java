/*
 * (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
 */

package com.palantir.conjure.verification;

import com.google.common.base.Preconditions;
import com.google.common.collect.ImmutableMap;
import com.google.common.io.MoreFiles;
import com.google.common.io.RecursiveDeleteOption;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.List;
import java.util.Map;

public final class GenerateClientServices {

    private GenerateClientServices() {}

    public static void main(String... args) throws IOException {
        Preconditions.checkArgument(args.length == 2, "Usage: <master-test-cases.yml> <gitignored output dir>");
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
    }

    private static void writeServiceDefinition(
            File fileName, String serviceName, Map<String, Object> service) throws IOException {
        TestCasesUtils.YAML_MAPPER.writeValue(fileName,
                createConjureYmlBuilder().put("services", ImmutableMap.of(serviceName, service)).build());
    }

    private static ImmutableMap.Builder<String, Object> createConjureYmlBuilder() {
        ImmutableMap.Builder<String, Object> builder = ImmutableMap.builder();
        builder.put(
                "types",
                ImmutableMap.of("conjure-imports",
                        ImmutableMap.of(
                                "examples", "../example-types.conjure.yml",
                                "testCases", "../test-cases.conjure.yml")));
        return builder;
    }

    private static Map<String, Object> generateAutoDeserializeService(List<BodyTests> bodyTests) {
        ImmutableMap.Builder<String, Object> endpoints = ImmutableMap.builder();

        bodyTests.stream().map(BodyTests::getType).map(TestCasesUtils::parseConjureType).forEach(conjureType -> {
            String endpointName = ClientTestCasesUtils.typeToEndpointName(conjureType);
            String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
            endpoints.put(endpointName, ImmutableMap.of(
                    "http", "POST /" + endpointName,
                    "returns", typeName,
                    "args", ImmutableMap.of("body", typeName)));
        });

        return ImmutableMap.of(
                "name", "Auto Deserialize Service",
                "package", "com.palantir.conjure.verification.client",
                "default-auth", "none",
                "base-path", "/body",
                "endpoints", endpoints.build());
    }
}
