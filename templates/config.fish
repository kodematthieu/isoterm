set fish_data_dir (string split ':' $XDG_DATA_DIRS)[1]

if status is-interactive
    starship init fish | source
    atuin init fish | source
    zoxide init fish | string replace --regex \
        -- '\$__fish_data_dir' $fish_data_dir | source
end

# ------------------------------------------------------------------------------
# SESSION HOOKS
# ------------------------------------------------------------------------------
function on_exit --on-event fish_exit
    echo "Exiting isolated shell environment."
end
