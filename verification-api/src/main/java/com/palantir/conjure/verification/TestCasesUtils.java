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
import java.io.File;
import java.io.IOException;

public final class TestCasesUtils {

    public static final ObjectMapper YAML_MAPPER =
            ObjectMappers.withDefaultModules(new ObjectMapper(new YAMLFactory()))
                    .enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);

    public static AllTestCases parseTestCases(File file) throws IOException {
        return YAML_MAPPER.readValue(file, AllTestCases.class);
    }
}
