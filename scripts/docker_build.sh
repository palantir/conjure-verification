#!/usr/bin/env bash
set -euxo pipefail
cd "$(dirname "${BASH_SOURCE[0]}" )"/..

case $(uname -s) in
    Darwin*) echo "docker images can only be built on linux (ie on CircleCI)" && exit 1 ;;
esac

VERSION=$(git describe --tags --always --first-parent)

function build_docker() (
    DEST="build/$1/docker-context"
    rm -rf "$DEST"
    mkdir -p "$DEST"

    cp "./verification-$1/Dockerfile" "$DEST/Dockerfile"

    BINARY="./target/release/conjure-verification-$1"
    if [ ! -f "$BINARY" ]; then
        echo "$BINARY must exist - run 'cargo build --release' to create it"
        exit 1
    fi
    cp "$BINARY" "$DEST"

    TEST_CASES=./verification-$1-api/build/test-cases.json
    if [ ! -f "$TEST_CASES" ]; then
        echo "$TEST_CASES file must exist - run './gradlew compileTestCasesJson' to create it"
        exit 1
    fi
    cp "$TEST_CASES" "$DEST"

    IR_FILE="./verification-$1-api/build/conjure-ir/verification-$1-api.conjure.json"
    if [ ! -f "$IR_FILE" ]; then
        echo "$IR_FILE file must exist - run './gradlew compileIr' to create it"
        exit 1
    fi
    cp "$IR_FILE" "$DEST"

    cd "$DEST"
    docker build -t "palantirtechnologies/conjure-verification-$1:$VERSION" .

    docker tag "palantirtechnologies/conjure-verification-$1:$VERSION" "palantirtechnologies/conjure-verification-$1:latest"
)

build_docker "server"
build_docker "client"
