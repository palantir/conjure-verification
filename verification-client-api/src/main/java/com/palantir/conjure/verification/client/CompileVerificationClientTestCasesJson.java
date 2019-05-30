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

package com.palantir.conjure.verification.client;

import static java.util.stream.Collectors.toSet;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.google.common.base.Preconditions;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.Sets;
import com.google.common.collect.Streams;
import com.google.common.io.MoreFiles;
import com.palantir.conjure.defs.Conjure;
import com.palantir.conjure.java.serialization.ObjectMappers;
import com.palantir.conjure.spec.ConjureDefinition;
import com.palantir.conjure.spec.ServiceDefinition;
import com.palantir.conjure.verification.BodyTests;
import com.palantir.conjure.verification.ConjureTypeString;
import com.palantir.conjure.verification.MasterTestCases;
import com.palantir.conjure.verification.TestCase;
import com.palantir.conjure.verification.TestCasesUtils;
import java.io.File;
import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public final class CompileVerificationClientTestCasesJson {

    private CompileVerificationClientTestCasesJson() {}

    public static void main(String... args) throws IOException {

        Preconditions.checkArgument(args.length == 2, "Usage: <master-test-cases.yml> <client-test-cases.json>");
        File file = new File(args[0]);
        File outputFile = new File(args[1]);

        MasterTestCases masterTestCases = TestCasesUtils.parseTestCases(file);

        TestCases testCases = TestCases.of(ServerTestCases.builder()
                .autoDeserialize(generateBodyTestCases(masterTestCases.getBody()))
                .build());

        ServerTestCases serverTestCases = testCases.getServer();

        ObjectMapper jsonMapper = ObjectMappers.newServerObjectMapper();

        long total = countPositiveAndNegative(serverTestCases.getAutoDeserialize());

        System.out.println("Total test cases: " + total);
        jsonMapper.writerWithDefaultPrettyPrinter().writeValue(outputFile, testCases);

        List<File> files = Streams
                .stream(MoreFiles.fileTraverser().breadthFirst(Paths.get("src/main/conjure")))
                .filter(MoreFiles.isDirectory().negate())
                .filter(path -> path.getFileName().toString().endsWith(".yml"))
                .map(Path::toFile)
                .collect(Collectors.toList());
        ConjureDefinition ir = Conjure.parse(files);

        checkEndpointNamesMatchPaths(ir);
        checkNoLeftovers(outputFile, serverTestCases.getAutoDeserialize().keySet(),
                serviceByName(ir, "AutoDeserializeService"));
    }

    private static long countPositiveAndNegative(Map<EndpointName, PositiveAndNegativeTestCases> tests) {
        return tests.entrySet().stream()
                .flatMap(e -> Stream.concat(e.getValue().getPositive().stream(), e.getValue().getNegative().stream()))
                .count();
    }

    private static void checkEndpointNamesMatchPaths(ConjureDefinition ir) {
        ir.getServices().forEach(service -> {
            String name = service.getServiceName().getName();

            service.getEndpoints().forEach(endpoint -> {
                if (!endpoint.getHttpPath().get().contains(endpoint.getEndpointName().get())) {
                    throw new RuntimeException(String.format(
                            "%s#%s has an inconsistent path: %s",
                            name,
                            endpoint.getEndpointName(),
                            endpoint.getHttpPath()));
                }
            });
        });
    }

    private static void checkNoLeftovers(
            File outputFile,
            Set<EndpointName> testCases,
            ServiceDefinition serviceDefinition) {
        Set<String> fromTestCasesYml = testCases.stream().map(EndpointName::get).collect(toSet());

        Set<String> realApiDefinition = serviceDefinition.getEndpoints().stream()
                .map(def -> def.getEndpointName().get())
                .collect(toSet());

        Sets.SetView<String> missing1 = Sets.difference(realApiDefinition, fromTestCasesYml);
        if (!missing1.isEmpty()) {
            throw new RuntimeException("Conjure API defines some endpoints but they are not used in the generated "
                    + outputFile + ": " + missing1);
        }

        Sets.SetView<String> missing2 = Sets.difference(fromTestCasesYml, realApiDefinition);
        if (!missing2.isEmpty()) {
            throw new RuntimeException("The generated " + outputFile + " mentions some endpoints, "
                    + "but they are not present in any conjure API definition: " + missing2);
        }
    }

    private static ServiceDefinition serviceByName(ConjureDefinition ir, String name) {
        return ir.getServices().stream().filter(s -> s.getServiceName().getName().equals(name)).findFirst().get();
    }

    private static Map<EndpointName, PositiveAndNegativeTestCases> generateBodyTestCases(List<BodyTests> bodyTests) {
        ImmutableMap.Builder<EndpointName, PositiveAndNegativeTestCases> builder = ImmutableMap.builder();
        bodyTests.forEach(t ->
                builder.put(endpointName("get", t.getType()),
                        PositiveAndNegativeTestCases
                                .builder()
                                .positive(t.getPositive().stream().map(TestCase::get).collect(Collectors.toList()))
                                .negative(t.getNegative().stream().map(TestCase::get).collect(Collectors.toList()))
                                .addAllNegative(t
                                        .getClientPositiveServerFail()
                                        .stream()
                                        .map(TestCase::get)
                                        .collect(Collectors.toList()))
                                .build()));
        return builder.build();
    }

    private static EndpointName endpointName(String prefix, ConjureTypeString type) {
        return EndpointName.of(
                TestCasesUtils.typeToEndpointName(prefix, TestCasesUtils.parseConjureType(type)));
    }
}
