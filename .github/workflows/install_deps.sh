#!/usr/bin/env bash

apt-get update
apt-get install -y clang
version=clang --version
echo $version