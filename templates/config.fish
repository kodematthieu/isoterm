if status is-interactive
    starship init fish | source
    atuin init fish | source
    zoxide init fish | source
end

function on_exit --on-event fish_exit
    echo "Exiting isolated shell environment."
end