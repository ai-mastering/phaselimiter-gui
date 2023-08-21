// +build aix android darwin dragonfly freebsd illumos ios linux netbsd openbsd plan9 solaris wasip1

package main

import (
	"os/exec"
)

func CmdHideWindow(cmd *exec.Cmd) {}
