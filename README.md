phaselimiter-gui is a GUI frontend for phaselimiter (a mastering program used in bakuage.com and aimastering.com).

## features

- The same algorithm with bakuage.com and aimastering.com
- No internet access (run on local PC)
- No settings and poor UI (under implementation)
- No support

## install

### windows

1. Install [Microsoft Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170) (not required if already installed)
2. Download the files attached to [the latest release](https://github.com/ai-mastering/phaselimiter-gui/releases) and extract it
3. Download [ffmpeg.exe](https://ffmpeg.org/) and put it in the same directory as phaselimiter-gui.exe (or install to some location in $PATH)
4. Run phaselimiter-gui.exe

### linux/mac

Please build it from the source code yourself.

Alternatively, although I haven't tested it, running the Windows binary using Wine might be an easier option.
This is especially the case because building the main automatic mastering program, phaselimiter, can be challenging due to the many dependencies required.

## how to use

Drop audio files to the app window

<img width="379" alt="スクリーンショット 2023-08-21 21 18 45" src="https://github.com/ai-mastering/phaselimiter-gui/assets/19356869/13e0c3d5-01a5-4acf-aad6-ba92cfb15c69">

## how to debug

Use phaselimiter-gui-console.exe that is the same with phaselimiter-gui.exe except it logs to the console

## runtime dependencies

### windows

- Microsoft Visual C++ Redistributable
- ffmpeg.exe

### linux/mac

Please build it from source code by yourself.

## license

- MIT
- thirdparty: https://github.com/ai-mastering/phaselimiter-gui/tree/master/licenses

## In Japanese

phaselimiter-gui は phaselimiter (bakuage.com / aimastering.com で使われているマスタリングプログラム) のGUIフロントエンドです

## 特徴

- bakuage.com / aimastering.com と同じマスタリングアルゴリズム
- インターネット不要 (ローカルPCで計算)
- 設定無し/poor UI (開発中)
- サポート無し

## インストール

### windows

1. [Microsoft Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170) をインストール (インストール済みの場合は不要)
2. https://github.com/ai-mastering/phaselimiter-gui/releases から最新ファイルをダウンロードし解凍
3. [ffmpeg.exe](https://ffmpeg.org/) をダウンロードし phaselimiter-gui.exe と同じディレクトリに格納 (またはパスの通った場所へインストール)
5. phaselimiter-gui.exe を実行

### linux/mac

ソースコードからビルド。

phaselimiterのビルドに必要な依存関係が多いので、動作確認していませんが、Windows用のバイナリをwineで動かすほうが簡単かもしれません。

## 使い方

アプリの画面にオーディオファイルをドロップ

<img width="379" alt="スクリーンショット 2023-08-21 21 18 45" src="https://github.com/ai-mastering/phaselimiter-gui/assets/19356869/13e0c3d5-01a5-4acf-aad6-ba92cfb15c69">

## デバッグ方法

phaselimiter-gui-console.exe を使うとログを確認可能
