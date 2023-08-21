// +build windows

package main

import (
	"os/exec"
	"syscall"
)

func CmdHideWindow(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
}
