package main

import (
	"fmt"
	"github.com/gotk3/gotk3/gdk"
	"github.com/gotk3/gotk3/glib"
	"github.com/gotk3/gotk3/gtk"
	"log"
	"net/url"
	"os"
	"path/filepath"
	"strings"
)

const (
	COLUMN_INPUT = iota
	COLUMN_OUTPUT
)

func getExecDir() string {
	ex, err := os.Executable()
	if err != nil {
		log.Fatal(err)
	}
	return filepath.Dir(ex)
}

func getDefaultOutputDir() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return "/tmp"
	}
	downloads := home + "/Downloads"
	_, err = os.Stat(downloads)
	if err == nil {
		return downloads
	}
	desktop := home + "/Desktop"
	_, err = os.Stat(desktop)
	if err == nil {
		return desktop
	}
	return home
}

func createTreeViewColumn(title string, order int) *gtk.TreeViewColumn {
	renderer, _ := gtk.CellRendererTextNew()
	tvc, _ := gtk.TreeViewColumnNewWithAttribute(
		title, renderer, "text", order)
	return tvc
}

func main() {
	masteringRunner := CreateMasteringRunner()
	go masteringRunner.Run()

	gtk.Init(nil)

	win, err := gtk.WindowNew(gtk.WINDOW_TOPLEVEL)
	if err != nil {
		log.Fatal("Unable to create window:", err)
	}
	win.SetTitle("phaselimiter-gui")
	win.SetDefaultSize(400, 400)
	win.Connect("destroy", func() {
		masteringRunner.Terminate()
		gtk.MainQuit()
	})

	targets, err := gtk.TargetEntryNew("text/uri-list", gtk.TARGET_OTHER_APP, 1)
	if err != nil {
		log.Fatal("Unable to create target entry:", err)
	}
	win.DragDestSet(gtk.DEST_DEFAULT_ALL, []gtk.TargetEntry{*targets}, gdk.ACTION_LINK)

	box, err := gtk.BoxNew(gtk.ORIENTATION_VERTICAL, 0)
	win.Add(box)

	entry_label, err := gtk.LabelNew("Output directory")
	box.Add(entry_label)
	entry, err := gtk.EntryNew()
	entry.SetText(getDefaultOutputDir())
	box.Add(entry)

	//mastering_scale_label, err := gtk.LabelNew("Mastering intensity")
	//box.Add(mastering_scale_label)
	//mastering_scale, err := gtk.ScaleNew(gtk.ORIENTATION_HORIZONTAL)
	//mastering_scale.SetRange(0, 1)
	//box.Add(mastering_scale)

	notes, err := gtk.LabelNew(`Drop audio files.

Process
1. The input audio files are mastered
2. The output files are saved to output directory

Notes
- Same algorithm with bakuage.com/aimastering.com
- No internet access`)
	box.Add(notes)

	ls, err := gtk.ListStoreNew(glib.TYPE_STRING, glib.TYPE_STRING)

	tv, err := gtk.TreeViewNewWithModel(ls)
	tv.AppendColumn(createTreeViewColumn("input file", COLUMN_INPUT))
	tv.AppendColumn(createTreeViewColumn("output file", COLUMN_OUTPUT))
	box.Add(tv)

	var destInData = func(lbi *gtk.Window,
		context *gdk.DragContext,
		x, y int,
		data_ptr *gtk.SelectionData,
		info, time uint) {

		m := Mastering{}
		m.Ffmpeg = "ffmpeg"
		m.PhaselimiterPath = filepath.Join(getExecDir(), "phaselimiter/bin/phase_limiter")
		m.SoundQuality2Cache = filepath.Join(getExecDir(), "phaselimiter/resource/sound_quality2_cache")

		m.Input = string(data_ptr.GetData())
		m.Input = strings.TrimPrefix(m.Input, "file://")
		m.Input = strings.TrimSuffix(m.Input, "\n")
		m.Input = strings.TrimSuffix(m.Input, "\r")
		m.Input, _ = url.QueryUnescape(m.Input)
		outputDir, _ := entry.GetText()
		m.Output = filepath.Join(outputDir, filepath.Base(m.Input)+".output.wav")

		//m.Level = mastering_scale.GetFillLevel()

		iter := ls.Append()
		ls.Set(iter, []int{COLUMN_INPUT, COLUMN_OUTPUT}, []interface{}{m.Input, m.Output})

		masteringRunner.Add(m)
	}
	win.Connect("drag-data-received", destInData)

	go func() {
		for {
			m := <-masteringRunner.MasteringUpdate
			fmt.Printf("%#v\n", m)
		}
	}()

	win.ShowAll()
	gtk.Main()
}
