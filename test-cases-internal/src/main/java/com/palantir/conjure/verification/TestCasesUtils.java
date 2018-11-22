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

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import com.palantir.conjure.java.serialization.ObjectMappers;
import com.palantir.conjure.parser.types.ConjureType;
import com.palantir.parsec.ParseException;
import java.io.File;
import java.io.IOException;
import org.apache.commons.lang3.StringUtils;

public final class TestCasesUtils {
    public static final ObjectMapper YAML_MAPPER = ObjectMappers
            .withDefaultModules(new ObjectMapper(new YAMLFactory()))
            .enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);

    private TestCasesUtils() {}

    public static MasterTestCases parseTestCases(File file) throws IOException {
        return YAML_MAPPER.readValue(file, MasterTestCases.class);
    }

    public static ConjureType parseConjureType(ConjureTypeString typeRepr) {
        ConjureType conjureType;
        try {
            conjureType = ConjureType.fromString(typeRepr.get());
        } catch (ParseException e) {
            throw new RuntimeException("Failed to parse conjure type: " + typeRepr.get(), e);
        }
        return conjureType;
    }

    public static String typeToEndpointName(String prefix, ConjureType type) {
        return prefix + StringUtils.capitalize(type.visit(new EndpointNameConjureTypeVisitor()));
    }
}
