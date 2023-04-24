#!/bin/bash

# read argument 1
if [[ $1 == "-d" ]]
then
    DEBUG=1
else
    DEBUG=0
fi

CONFIG_LOCAL=./config.txt
PROGRAM=project3

if [[ $DEBUG -eq 1 ]]
then
    cargo build || exit 1
    BINARY_DIR=./target/debug/
else
    cargo build -r || exit 1
    BINARY_DIR=./target/release/
fi

n=0
cat $CONFIG_LOCAL | sed -e "s/#.*//" | sed -e "/^\s*$/d" |
(
    read i
    while [[ $n -lt $i ]]
    do
        read line
        p=$( echo $line | awk '{ print $1 }' )
        host=$( echo $line | awk '{ print $2 }' )
        # export RUST_BACKTRACE=1
        kitty --hold -e $BINARY_DIR/$PROGRAM $CONFIG_LOCAL $p &
        n=$(( n + 1 ))
    done
    wait
)
