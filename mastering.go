package main

import (
	"bufio"
	"io"
	"os/exec"
	"regexp"
	"strconv"
)

type MasteringStatus int

const (
	MasteringStatusWaiting MasteringStatus = iota
	MasteringStatusProcessing
	MasteringStatusFailed
	MasteringStatusSucceeded
)

type Mastering struct {
	id                 int
	Input              string
	Output             string
	Ffmpeg             string
	PhaselimiterPath   string
	SoundQuality2Cache string
	//Level              float64
	Progression float64
	Status      MasteringStatus
	Message     string
}

type MasteringRunner struct {
	MasteringUpdate chan Mastering
	mastering       chan Mastering
	terminated      chan bool
}

func (m Mastering) execute(update chan Mastering) {
	args := []string{
		"--input", m.Input,
		"--output", m.Output,
		"--ffmpeg", m.Ffmpeg,
		"--mastering", "true",
		"--mastering_mode", "mastering5",
		"--sound_quality2_cache", m.SoundQuality2Cache,
	}
	cmd := exec.Command(m.PhaselimiterPath, args...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		m.Status = MasteringStatusFailed
		m.Message = "failed to create stdout pipe: " + err.Error()
		update <- m
		return
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		m.Status = MasteringStatusFailed
		m.Message = "failed to create stderr pipe: " + err.Error()
		update <- m
		return
	}

	m.Status = MasteringStatusProcessing
	update <- m

	err = cmd.Start()
	if err != nil {
		m.Status = MasteringStatusFailed
		m.Message = "failed to start command: " + err.Error()
		update <- m
		return
	}

	merged := io.MultiReader(stderr, stdout)
	scanner := bufio.NewScanner(merged)
	r := regexp.MustCompile("progression: ([-+]?[0-9]*\\.?[0-9]+)")
	output := ""
	for scanner.Scan() {
		line := scanner.Text()
		output += line
		matches := r.FindStringSubmatch(line)
		if len(matches) > 0 {
			m.Progression, _ = strconv.ParseFloat(matches[1], 64)
			update <- m
		}
	}

	err = cmd.Wait()
	if err != nil {
		m.Status = MasteringStatusFailed
		m.Message = "command failed: " + err.Error() + " output: " + output
		update <- m
		return
	}

	m.Progression = 1
	m.Status = MasteringStatusSucceeded
	update <- m
}

func CreateMasteringRunner() MasteringRunner {
	m := MasteringRunner{}
	m.mastering = make(chan Mastering, 1000)
	m.terminated = make(chan bool, 1000)
	m.MasteringUpdate = make(chan Mastering, 1000)
	return m
}

func (m MasteringRunner) Run() {
	for {
		select {
		case x := <-m.mastering:
			x.execute(m.MasteringUpdate)
		case _ = <-m.terminated:
			return
		}
	}
}

func (m MasteringRunner) Add(mastering Mastering) {
	m.mastering <- mastering
}

func (m MasteringRunner) Terminate() {
	m.terminated <- true
}