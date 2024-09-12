# `pidtree_mon` - a CPU load monitor of process trees

This utility monitors selected processes' CPU usage calculated as the total CPU usage for the whole
process subtree. The process subtree is defined as the subtree of the process forest calculated
according to the parent-child relationship between two PIDs.

It is useful for example if you want to know the total CPU usage of your session, which you can
display in [tmux](https://github.com/tmux/tmux) using the following configuration:

```sh
%hidden WINDOW_LOAD="#( \
    pidtree_mon \
        -f sum \
        \$(tmux list-panes -t #{window_id} -F '##{pane_pid}') \
)"
set -wg window-status-format "#I:#W#F [$WINDOW_LOAD]"
set -wg window-status-current-format "#I:#W#F [$WINDOW_LOAD]"
```

> [!TIP]
> I haven't really found tmux to be very good at garbage-collecting the processes it spawns
> per-window, so I actually recommend adding for example `-t 10` to limit the duration of the
> monitor command. It appears that tmux will promptly re-spawn a process that exited, so it
> shouldn't be noticeable and will spare some zombie processes.

## Usage

```
Usage: pidtree_mon [OPTIONS] <pid>...

Arguments:
  <pid>...

Options:
  -t, --timeout <TIMEOUT>
  -f, --field <field>      sum | all_loads | if_greater:value:then[:else] [default: sum all_loads]
  -h, --help               Print help
  -V, --version            Print version
```

The specified fields are printed space-separated in each line. Each line represents one measurement.
Available fields:

| field                          | description                                                   |
| :----------------------------- | :------------------------------------------------------------ |
| `sum`                          | sum of CPU loads for all subtrees rooted in passed `<pid>...` |
| `all_loads`                    | loads of each subtree, separated by space                     |
| `if_greater:value:then[:else]` | `then` if `sum` > `value`, `else` otherwise                   |


## Why?

This project was created as a result of poor performance of the following solution to present an
icon for each window in [tmux](https://github.com/tmux/tmux) if the CPU usage of all the processes
spawned inside all panes of this window was above a threshold:

```sh
#!/bin/sh

CONDITION=">50"

WID=$1
while true; do
        PANE_PIDS=$(tmux list-panes -t ${WID} -F '#{pane_pid}')
        ALL_PIDS=$(ps --forest -o pid= -g $(echo $PANE_PIDS | xargs | tr ' ' ,))
        echo $(\
                ps -o %cpu= -p $(echo $ALL_PIDS | tr ' ' ,) \
                        | xargs \
                        | tr ' ' + \
        ) "$CONDITION" \
                | bc \
                | sed -n -e 's/1//p' -e 's/0/:/p'
        sleep 5
done
```

This script could be run in tmux like this:

```sh
%hidden WINDOW_FIRE="#[fg=red]#(/some/path/tmux_window_fire.sh #{window_id})#[fg=black]"
set -wg window-status-format "#I$WINDOW_FIRE#W#F"
set -wg window-status-current-format "#I$WINDOW_FIRE#W#F"
```

It appears that this is actually rather slow, as it got spawned for about 50 tmux windows, the whole
solution started using some 30% of all the computing power I had.

Apparently the reason this is slow is because reading information from `/proc` filesystem isn't very
fast, especially if you need to enumerate all the processes (that's what `ps --forest` does to
calculate the process trees). And having to calculate the loads for a number of windows meant doing
the heavy work a number of times.

So `pidtree_mon` is a solution to this problem. It optimizes this task by spawining a daemon that
computes the process forest once a second and calculates total loads for all subtrees, and then
allows clients to read the data they are interested in on demand.

The exact counterpart of the above configuration using `pidtree_mon` would be:

```sh
%hidden WINDOW_FIRE="#[fg=red]#(\
        pidtree_mon \
            -t 10 \
            -f 'if_greater:0.5:::' \
            \$(tmux list-panes -t #{window_id} -F '##{pane_pid}') \
)#[fg=black]"
set -wg window-status-format "#I$WINDOW_FIRE#W#F"
set -wg window-status-current-format "#I$WINDOW_FIRE#W#F"
```

