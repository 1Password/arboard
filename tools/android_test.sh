#!/bin/bash

# This script is used in CI to run a sample Android app
# and verify arboard implementation for the platform. 

# Make sure the package is removed since it may end up in the AVD cache. This causes
# INSTALL_FAILED_UPDATE_INCOMPATIBLE errors when the debug keystore is regenerated,
# as it is not stored/cached on the CI
adb uninstall rust.example.android || true

cargo apk run --target x86_64-linux-android --example android --no-logcat

sleep 30

adb logcat *:I -d > ~/logcat.log

if grep 'app started' ~/logcat.log;
then
    echo "App running"
else
    echo "::error::App not running"
    exit 1
fi

ERROR_MSG=$(grep -e "thread '.*' panicked at" ~/logcat.log)
if [ -z "${ERROR_MSG}" ];
then
    exit 0
else
    echo "::error::${ERROR_MSG}"
    exit 1
fi
