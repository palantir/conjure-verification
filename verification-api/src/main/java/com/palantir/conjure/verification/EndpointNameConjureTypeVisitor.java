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

import com.palantir.conjure.parser.types.ConjureType;
import com.palantir.conjure.parser.types.ConjureTypeVisitor;
import com.palantir.conjure.parser.types.builtin.AnyType;
import com.palantir.conjure.parser.types.builtin.BinaryType;
import com.palantir.conjure.parser.types.builtin.DateTimeType;
import com.palantir.conjure.parser.types.collect.ListType;
import com.palantir.conjure.parser.types.collect.MapType;
import com.palantir.conjure.parser.types.collect.OptionalType;
import com.palantir.conjure.parser.types.collect.SetType;
import com.palantir.conjure.parser.types.primitive.PrimitiveType;
import com.palantir.conjure.parser.types.reference.ForeignReferenceType;
import com.palantir.conjure.parser.types.reference.LocalReferenceType;
import org.apache.commons.lang3.StringUtils;

/**
 * Convert the {@link ConjureType} into a string that can be used as an endpoint name (without special characters).
 */
public final class EndpointNameConjureTypeVisitor implements ConjureTypeVisitor<String> {
    @Override
    public String visitAny(AnyType type) {
        return "any";
    }

    @Override
    public String visitList(ListType type) {
        return "listOf" + StringUtils.capitalize(type.itemType().visit(this));
    }

    @Override
    public String visitMap(MapType type) {
        return "mapOf" + StringUtils.capitalize(type.keyType().visit(this))
                + "To" + StringUtils.capitalize(type.valueType().visit(this));
    }

    @Override
    public String visitOptional(OptionalType type) {
        return "optionalOf" + StringUtils.capitalize(type.itemType().visit(this));
    }

    @Override
    public String visitPrimitive(PrimitiveType type) {
        return type.type().name();
    }

    @Override
    public String visitLocalReference(LocalReferenceType type) {
        return type.type().name();
    }

    @Override
    public String visitForeignReference(ForeignReferenceType type) {
        throw new UnsupportedOperationException(
                "Verification endpoints do not support foreign references: " + type.toString());
    }

    @Override
    public String visitSet(SetType type) {
        return "setOf" + StringUtils.capitalize(type.itemType().visit(this));
    }

    @Override
    public String visitBinary(BinaryType type) {
        return "binary";
    }

    @Override
    public String visitDateTime(DateTimeType type) {
        return "datetime";
    }
}
