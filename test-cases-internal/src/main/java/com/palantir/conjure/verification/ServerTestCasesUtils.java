/*
 * (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
 */

package com.palantir.conjure.verification;

import com.palantir.conjure.parser.types.ConjureType;
import org.apache.commons.lang3.StringUtils;

public final class ServerTestCasesUtils {
    private ServerTestCasesUtils() {
    }

    public static String typeToEndpointName(ConjureType type) {
        return "test" + StringUtils.capitalize(type.visit(new EndpointNameConjureTypeVisitor()));
    }
}
