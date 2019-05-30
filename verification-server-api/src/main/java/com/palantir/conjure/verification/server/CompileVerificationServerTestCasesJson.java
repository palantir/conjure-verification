/*
 * (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
 */

package com.palantir.conjure.verification.server;

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
import com.palantir.conjure.verification.SingleHeaderParamTests;
import com.palantir.conjure.verification.SinglePathParamTests;
import com.palantir.conjure.verification.SingleQueryParamTests;
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

public final class CompileVerificationServerTestCasesJson {

    private CompileVerificationServerTestCasesJson() {}

    public static void main(String... args) throws IOException {

        Preconditions.checkArgument(args.length == 2, "Usage: <master-test-cases.yml> <server-test-cases.json>");
        File file = new File(args[0]);
        File outputFile = new File(args[1]);

        MasterTestCases masterTestCases = TestCasesUtils.parseTestCases(file);

        TestCases testCases = TestCases.of(ClientTestCases.builder()
                .autoDeserialize(generateBodyTestCases(masterTestCases.getBody()))
                .singleHeaderService(generateSingleHeaderParamTestCases(masterTestCases.getSingleHeaderParam()))
                .singleQueryParamService(generateSingleQueryParamTestCases(masterTestCases.getSingleQueryParam()))
                .singlePathParamService(generateSinglePathParamTestCases(masterTestCases.getSinglePathParam()))
                .build());

        ClientTestCases clientTestCases = testCases.getClient();

        ObjectMapper jsonMapper = ObjectMappers.newServerObjectMapper();

        long total = countPositiveAndNegative(clientTestCases.getAutoDeserialize())
                + countTestCases(clientTestCases.getSingleHeaderService())
                + countTestCases(clientTestCases.getSinglePathParamService())
                + countTestCases(clientTestCases.getSingleQueryParamService());

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
        checkNoLeftovers(outputFile, clientTestCases.getAutoDeserialize().keySet(),
                serviceByName(ir, "AutoDeserializeService"));
        checkNoLeftovers(outputFile, clientTestCases.getSingleHeaderService().keySet(),
                serviceByName(ir, "SingleHeaderService"));
        checkNoLeftovers(outputFile, clientTestCases.getSinglePathParamService().keySet(),
                serviceByName(ir, "SinglePathParamService"));
        checkNoLeftovers(outputFile, clientTestCases.getSingleQueryParamService().keySet(),
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
            File outputFile, Set<EndpointName> testCases, ServiceDefinition serviceDefinition) {
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
                builder.put(endpointName("receive", t.getType()),
                        PositiveAndNegativeTestCases
                                .builder()
                                .positive(t.getPositive().stream().map(TestCase::get).collect(Collectors.toList()))
                                .negative(t.getNegative().stream().map(TestCase::get).collect(Collectors.toList()))
                                .addAllPositive(t
                                        .getClientPositiveServerFail()
                                        .stream()
                                        .map(TestCase::get)
                                        .collect(Collectors.toList()))
                                .build()));
        return builder.build();
    }

    private static Map<EndpointName, List<String>> generateSingleHeaderParamTestCases(
            List<SingleHeaderParamTests> singleHeaderParam) {
        ImmutableMap.Builder<EndpointName, List<String>> builder = ImmutableMap.builder();
        singleHeaderParam.forEach(t -> builder.put(
                endpointName("header", t.getType()),
                t.getPositive().stream().map(TestCase::get).collect(Collectors.toList())));
        return builder.build();
    }

    private static Map<EndpointName, List<String>> generateSingleQueryParamTestCases(
            List<SingleQueryParamTests> singleQueryParam) {
        ImmutableMap.Builder<EndpointName, List<String>> builder = ImmutableMap.builder();
        singleQueryParam.forEach(t -> builder.put(
                endpointName("queryParam", t.getType()),
                t.getPositive().stream().map(TestCase::get).collect(Collectors.toList())));
        return builder.build();
    }

    private static Map<EndpointName, List<String>> generateSinglePathParamTestCases(
            List<SinglePathParamTests> singlePathParam) {
        ImmutableMap.Builder<EndpointName, List<String>> builder = ImmutableMap.builder();
        singlePathParam.forEach(t -> builder.put(
                endpointName("pathParam", t.getType()),
                t.getPositive().stream().map(TestCase::get).collect(Collectors.toList())));
        return builder.build();
    }

    private static EndpointName endpointName(String prefix, ConjureTypeString type) {
        return EndpointName.of(TestCasesUtils.typeToEndpointName(prefix, TestCasesUtils.parseConjureType(type)));
    }
}
