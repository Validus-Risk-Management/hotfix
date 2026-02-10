package main

import (
	"dummy-executor/cmd"
	"log"
	"os"
)

func main() {
	cfgFileName := "config/executor.cfg"
	if len(os.Args) > 1 {
		cfgFileName = os.Args[1]
	}
	if err := cmd.Run(cfgFileName); err != nil {
		log.Fatalf("Error: %v", err)
	}
}
