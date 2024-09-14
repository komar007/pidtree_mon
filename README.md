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
> per-window, so I actually recommend adding for example `-t 60` to limit the duration of the
> monitor command. It appears that tmux will promptly re-spawn a process that exited, so it
> shouldn't be noticeable and will spare some zombie processes.

## Usage

```
A CPU load monitor of process trees

Usage: pidtree_mon [OPTIONS] <pid>...

Arguments:
  <pid>...  The collection of PIDs to monitor

Options:
  -t, --timeout <TIMEOUT>      The maximum time to collect statistics
  -f, --field <field>          sum[_t][:FMT] | all_loads[_t][:FMT] | TEST
                               FMT := .N | %N | TEST
                               TEST := if_range:[L]..[H]:then[:else] | if_greater:thr:then[:else]
                                [default: sum all_loads]
  -s, --separator <SEPARATOR>  The field separator [default: " "]
  -h, --help                   Print help
  -V, --version                Print version

Explanation of fields

Multiple fields can be passed via -f/--field. A basic field can be:
 * `sum' - sum of loads of all provided process trees,
 * `all_loads' - produces multiple fields, one for each process tree.

The values are scaled per-core, so n means n whole cores are being used.
Adding `_t' to either field scales the loads according to the total computing power,
1 being the maximum.

A format specifier can be added after colon:
 * .N - prints with N digits after decimal point,
 * %N - prints with N digits after decimal point, scaled up by a factor of 100,
 * if_range:[L]..[H]:then[:else] - produces `then` if field value is between `L' and `H',
                                   `else` otherwise, `L`, `H` and `else` are optional,
 * if_greater:thr:then[:else]    - like if_range, but field value must be greater than `thr`,
                                   DEPRECATED

Additionally, the last two specifiers can be used alone, without a preceding value,
in this case, the value defaults to `sum`.
```

## Examples

### print load status using different characters
```sh
pidtree_mon -s '' \
    -f 'sum:if_range:0.4..1.5: ' \
    -f 'sum:if_range:1.5..:🔥' \
    <pids>
```

Here we use per-core loads, mainly to detect potential single-core tight-loops in a session
(40%..150% range). Above 150%, the more serious emoji is used.

### print whole system's load as a vertical bar
```sh
    pidtree_mon
        -s '' \
        -f 'sum_t:if_range:0.000..0.050: ' \
        -f 'sum_t:if_range:0.050..0.125:#[fg=#B2E0B2]▁' \
        -f 'sum_t:if_range:0.125..0.250:#[fg=#A3D6A3]▂' \
        -f 'sum_t:if_range:0.250..0.375:#[fg=#94CC94]▃' \
        -f 'sum_t:if_range:0.375..0.500:#[fg=#85C285]▄' \
        -f 'sum_t:if_range:0.500..0.625:#[fg=#FFD6A0]▅' \
        -f 'sum_t:if_range:0.625..0.750:#[fg=#FFB3A0]▆' \
        -f 'sum_t:if_range:0.750..0.875:#[fg=#FF8C94]▇' \
        -f 'sum_t:if_range:0.875..1.500:#[fg=#FF6F61]█' \
        1 \
```

This command can be used directly in tmux with `#()`. It uses the total processor usage by PID 1,
which is not ideal, but will be good enough most of the time.

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
            -t 60 \
            -f 'if_greater:0.5:::' \
            \$(tmux list-panes -t #{window_id} -F '##{pane_pid}') \
)#[fg=black]"
set -wg window-status-format "#I$WINDOW_FIRE#W#F"
set -wg window-status-current-format "#I$WINDOW_FIRE#W#F"
```

