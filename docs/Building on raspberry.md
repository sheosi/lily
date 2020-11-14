# Building on a Raspberry

NOTE: Those may be incorrect, file an issue if you find some correction.

Building on a Raspberry pi has some extra steps, since some of the Python dependencies
are not compiled by default we need to take some extra steps. We assume here
that you are using Debian/Raspberry OS/Ubuntu.


Install system dependencies for snips-nlu dependencies:

```
sudo apt install gfortran libopenblas-dev liblapack-dev (no hace falta atlas?)
```

Also, you'll need to install Cython by yourself (needed by snips-nlu):

```
pip3 install --user cython
```

## Rasa NLU

If you are going to use the Rasa Nlu backend you'll need to do this:

```
sudo apt install libpq-dev
```

## Bluetooth

If you are going to use a Bluetooth speaker you'll need:

```
sudo apt install pulseaudio-module-bluetooth gstreamer1.0-pulseaudio
```

And overall just follow this [https://gist.github.com/actuino/9548329d1bba6663a63886067af5e4cb](https://gist.github.com/actuino/9548329d1bba6663a63886067af5e4cb)
