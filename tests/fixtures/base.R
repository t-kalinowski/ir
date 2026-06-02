#!/usr/bin/env -S ir run
# dependencies: []
# R: >= 4.0

cat(paste(.libPaths()[[1L]], paste(commandArgs(trailingOnly = TRUE), collapse = ","), sep = "\n"))
