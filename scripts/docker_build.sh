#!/usr/bin/env bash
set -ex

cd "$(dirname "${BASH_SOURCE[0]}" )"/..

mkdir -p build/docker-context

cp -R ./target/debug/conjure-verification-server build/docker-context/conjure-verification-server

cp ./verification-server/Dockerfile ./build/docker-context/Dockerfile

cd build/docker-context

docker build .
