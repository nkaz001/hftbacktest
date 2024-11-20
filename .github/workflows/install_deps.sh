#!/usr/bin/env bash

#apt-get update
#apt-get install -y clang
yum update -y
yum install -y epel-release
yum install -y clang
version=clang --version
echo $version