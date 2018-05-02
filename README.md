# MPD forwarder (async version)



## Run with:

    $ cargo watch -x run

## Send cmd with:

    $ echo "status" | telnet localhost 6601

## Check MPD log

    $ tail -f ~/.mpd/mpd.log

## Use telnet

    $ telnet localhost 6601
    status
    <CTRL + ]>
    telnet> quit
