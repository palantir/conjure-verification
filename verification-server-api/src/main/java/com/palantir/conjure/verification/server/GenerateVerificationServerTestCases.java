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

package com.palantir.conjure.verification.server;

import com.google.common.base.Preconditions;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableMap.Builder;
import com.palantir.conjure.verification.AllTestCases;
import com.palantir.conjure.verification.BodyTests;
import com.palantir.conjure.verification.ConjureTypeRepr;
import com.palantir.conjure.verification.SingleHeaderParamTests;
import com.palantir.conjure.verification.SinglePathParamTests;
import com.palantir.conjure.verification.SingleQueryParamTests;
import com.palantir.conjure.verification.TestCase;
import com.palantir.conjure.verification.TestCasesUtils;
import java.io.File;
import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

public final class GenerateVerificationServerTestCases {
    private GenerateVerificationServerTestCases() {}

    public static void main(String... args) throws IOException {
        Preconditions.checkArgument(args.length == 2, "Usage: <test-cases.yml> <server-test-cases.yml>");
        File file = new File(args[0]);
        File outputFile = new File(args[1]);

        AllTestCases allTestCases = TestCasesUtils.parseTestCases(file);

        TestCases testCases = TestCases.of(ClientTestCases.builder()
                .autoDeserialize(generateBodyTestCases(allTestCases.getBody()))
                .singleHeaderService(generateSingleHeaderParamTestCases(allTestCases.getSingleHeaderParam()))
                .singleQueryParamService(generateSingleQueryParamTestCases(allTestCases.getSingleQueryParam()))
                .singlePathParamService(generateSinglePathParamTestCases(allTestCases.getSinglePathParam()))
                .build());

        TestCasesUtils.YAML_MAPPER.writeValue(outputFile, testCases);
    }

    private static Map<EndpointName, PositiveAndNegativeTestCases> generateBodyTestCases(List<BodyTests> bodyTests) {
        Builder<EndpointName, PositiveAndNegativeTestCases> builder = ImmutableMap.builder();
        bodyTests.forEach(t ->
                builder.put(endpointName(t.getType()),
                PositiveAndNegativeTestCases
                        .builder()
                        .positive(t.getBothPositive().stream().map(TestCase::get).collect(Collectors.toList()))
                        .addAllPositive(t
                                .getClientPositiveServerFail()
                                .stream()
                                .map(TestCase::get)
                                .collect(Collectors.toList()))
                        .negative(t.getBothNegative().stream().map(TestCase::get).collect(Collectors.toList()))
                        .build()));
        return builder.build();
    }

    private static Map<EndpointName, List<String>> generateSingleHeaderParamTestCases(
            List<SingleHeaderParamTests> singleHeaderParam) {
        Builder<EndpointName, List<String>> builder = ImmutableMap.builder();
        singleHeaderParam.forEach(t -> builder.put(
                endpointName(t.getType()),
                t.getBothPositive().stream().map(TestCase::get).collect(Collectors.toList())));
        return builder.build();
    }

    private static Map<EndpointName, List<String>> generateSingleQueryParamTestCases(
            List<SingleQueryParamTests> singleQueryParam) {
        Builder<EndpointName, List<String>> builder = ImmutableMap.builder();
        singleQueryParam.forEach(t -> builder.put(
                endpointName(t.getType()),
                t.getBothPositive().stream().map(TestCase::get).collect(Collectors.toList())));
        return builder.build();
    }

    private static Map<EndpointName, List<String>> generateSinglePathParamTestCases(
            List<SinglePathParamTests> singlePathParam) {
        Builder<EndpointName, List<String>> builder = ImmutableMap.builder();
        singlePathParam.forEach(t -> builder.put(
                endpointName(t.getType()),
                t.getBothPositive().stream().map(TestCase::get).collect(Collectors.toList())));
        return builder.build();
    }

    private static EndpointName endpointName(ConjureTypeRepr type) {
        return EndpointName.of(
                ServerTestCasesUtils.typeToEndpointName(TestCasesUtils.parseConjureType(type)));
    }
}
