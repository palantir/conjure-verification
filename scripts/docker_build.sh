#!/usr/bin/env bash
set -ex
cd "$(dirname "${BASH_SOURCE[0]}" )"/..

case $(uname -s) in
    Linux*) BINARY=./target/release/conjure-verification-server ;;
    Darwin*) echo "unable to build linux docker image on mac" && exit 1 ;;
esac

if [ -f $BINARY ]; then
    echo "$BINARY must exist - run 'cargo build --release' to create it"
    exit 1
fi

VERSION=$(git describe --tags --always --first-parent)
DEST=build/docker-context

rm -rf $DEST
mkdir -p $DEST

cp -R $BINARY $DEST

cp ./verification-server/Dockerfile $DEST/Dockerfile

cd $DEST

docker build -t "palantirtechnologies/conjure-verification-server:$VERSION" .
