/*
 * (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
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

public final class GenerateClientServices {

    private GenerateClientServices() {}

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
                    String endpointName = TestCasesUtils.typeToEndpointName("get", conjureType);
                    String typeName = conjureType.visit(new ResolveLocalReferencesConjureTypeVisitor());
                    endpoints.put(
                            endpointName,
                            ImmutableMap.of(
                                    "http",
                                    "POST /" + endpointName,
                                    "returns",
                                    typeName,
                                    "args",
                                    ImmutableMap.of("body", typeName)));
                });

        return ImmutableMap.of(
                "name", "Auto Deserialize Service",
                "package", "com.palantir.conjure.verification.client",
                "default-auth", "none",
                "base-path", "/body",
                "endpoints", endpoints.build());
    }
}
