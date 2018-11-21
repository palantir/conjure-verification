/*
 * (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
 */

package com.palantir.conjure.verification.server;

import static java.util.stream.Collectors.toSet;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import com.google.common.collect.Sets;
import com.google.common.collect.Streams;
import com.google.common.io.MoreFiles;
import com.palantir.conjure.defs.Conjure;
import com.palantir.conjure.java.serialization.ObjectMappers;
import com.palantir.conjure.spec.ConjureDefinition;
import com.palantir.conjure.spec.ServiceDefinition;
import java.io.File;
import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public final class CompileVerificationServerTestCasesJson {

    private CompileVerificationServerTestCasesJson() {}

    public static void main(String... args) throws IOException {

        TestCases testCases = ObjectMappers
                .withDefaultModules(new ObjectMapper(new YAMLFactory()))
                .readValue(new File("test-cases.yml"), TestCases.class);

        ClientTestCases clientTestCases = testCases.getClient();

        ObjectMapper jsonMapper = ObjectMappers.newServerObjectMapper();

        long total = countPositiveAndNegative(clientTestCases.getAutoDeserialize())
                + countTestCases(clientTestCases.getSingleHeaderService())
                + countTestCases(clientTestCases.getSinglePathParamService())
                + countTestCases(clientTestCases.getSingleQueryParamService());

        System.out.println("Total test cases: " + total);
        jsonMapper.writerWithDefaultPrettyPrinter().writeValue(new File("build/test-cases.json"), testCases);

        List<File> files = Streams
                .stream(MoreFiles.fileTraverser().breadthFirst(Paths.get("src/main/conjure")))
                .filter(MoreFiles.isDirectory().negate())
                .filter(path -> path.getFileName().toString().endsWith(".yml"))
                .map(Path::toFile)
                .collect(Collectors.toList());
        ConjureDefinition ir = Conjure.parse(files);

        checkEndpointNamesMatchPaths(ir);
        checkNoLeftovers(clientTestCases.getAutoDeserialize().keySet(),
                serviceByName(ir, "AutoDeserializeService"));
        checkNoLeftovers(clientTestCases.getSingleHeaderService().keySet(),
                serviceByName(ir, "SingleHeaderService"));
        checkNoLeftovers(clientTestCases.getSinglePathParamService().keySet(),
                serviceByName(ir, "SinglePathParamService"));
        checkNoLeftovers(clientTestCases.getSingleQueryParamService().keySet(),
                serviceByName(ir, "SingleQueryParamService"));
    }

    private static long countTestCases(Map<EndpointName, List<String>> tests) {
        return tests.entrySet().stream()
                .flatMap(e -> e.getValue().stream())
                .count();
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
