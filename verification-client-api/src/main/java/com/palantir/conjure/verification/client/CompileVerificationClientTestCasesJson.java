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

import com.fasterxml.jackson.annotation.JsonInclude;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import com.google.common.collect.Sets;
import com.palantir.conjure.defs.Conjure;
import com.palantir.conjure.java.serialization.ObjectMappers;
import com.palantir.conjure.spec.ConjureDefinition;
import com.palantir.conjure.spec.ServiceDefinition;
import java.io.File;
import java.io.IOException;
import java.util.Arrays;
import java.util.Map;
import java.util.Set;
import java.util.stream.Stream;

public final class CompileVerificationClientTestCasesJson {

    private CompileVerificationClientTestCasesJson() {}

    public static void main(String... args) throws IOException {

        TestCases testCases = ObjectMappers
                .withDefaultModules(new ObjectMapper(new YAMLFactory()))
                .readValue(new File("test-cases.yml"), TestCases.class);

        ServerTestCases serverTestCases = testCases.getServer();

        ObjectMapper jsonMapper = ObjectMappers.newServerObjectMapper()
                .setSerializationInclusion(JsonInclude.Include.NON_NULL)
                .setSerializationInclusion(JsonInclude.Include.NON_EMPTY);

        long total = countPositiveAndNegative(serverTestCases.getAutoDeserialize());

        System.out.println("Total test cases: " + total);
        jsonMapper.writerWithDefaultPrettyPrinter().writeValue(new File("build/test-cases.json"), testCases);

        File dir = new File("src/main/conjure");
        ConjureDefinition ir = Conjure.parse(Arrays.asList(dir.listFiles()));

        checkEndpointNamesMatchPaths(ir);
        checkNoLeftovers(serverTestCases.getAutoDeserialize().keySet(),
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
            Set<EndpointName> testCases,
            ServiceDefinition serviceDefinition) {

        Set<String> fromTestCasesYml = testCases.stream().map(EndpointName::get).collect(toSet());

        Set<String> realApiDefinition = serviceDefinition.getEndpoints().stream()
                .map(def -> def.getEndpointName().get())
                .collect(toSet());

        Sets.SetView<String> missing1 = Sets.difference(realApiDefinition, fromTestCasesYml);
        if (!missing1.isEmpty()) {
            throw new RuntimeException("Conjure API defines some endpoints but they are not used in test-cases.yml: "
                    + missing1);
        }

        Sets.SetView<String> missing2 = Sets.difference(fromTestCasesYml, realApiDefinition);
        if (!missing2.isEmpty()) {
            throw new RuntimeException("test-cases.yml mentions some endpoints, "
                    + "but they are not present in any conjure API definition: " + missing2);
        }
    }

    private static ServiceDefinition serviceByName(ConjureDefinition ir, String name) {
        return ir.getServices().stream().filter(s -> s.getServiceName().getName().equals(name)).findFirst().get();
    }
}
