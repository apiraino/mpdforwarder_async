## MPD forwarder (async version)

 A simple proxy that forwards TCP payload to an MPD server.

 This is the async version, using the tokio library.

A pet project to give a first glance at the Tokio library (basically some copy and paste from the tutorial).

### Run with:

    $ cargo watch -x run

### Send cmd with:

    $ echo "status" | ncat 127.0.0.1 6601

### Check MPD log

    $ tail -f ~/.mpd/mpd.log

### Use telnet

    $ telnet localhost 6601
    status
    <CTRL + ]>
    telnet> quit
