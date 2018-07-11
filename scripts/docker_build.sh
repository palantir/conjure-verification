#!/usr/bin/env bash
set -ex
cd "$(dirname "${BASH_SOURCE[0]}" )"/..

case $(uname -s) in
    Darwin*) echo "docker images can only be built on linux (ie on CircleCI)" && exit 1 ;;
esac

VERSION=$(git describe --tags --always --first-parent)
DEST=build/docker-context
rm -rf $DEST
mkdir -p $DEST

cp ./verification-server/Dockerfile $DEST/Dockerfile

BINARY=./target/release/conjure-verification-server
if [ ! -f $BINARY ]; then
    echo "$BINARY must exist - run 'cargo build --release' to create it"
    exit 1
fi
cp $BINARY $DEST

TEST_CASES=./verification-api/build/test-cases.json
if [ ! -f $TEST_CASES ]; then
    echo "$TEST_CASES file must exist - run './gradlew compileTestCasesJson' to create it"
    exit 1
fi
cp $TEST_CASES $DEST

cd $DEST
docker build -t "palantirtechnologies/conjure-verification-server:$VERSION" .
