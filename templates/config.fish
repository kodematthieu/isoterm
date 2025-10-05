# Determine the root directory of the isoterm environment.
# (status --current-filename) gives the path to this script.
# We navigate up from .../config/fish to the root.
set -l ISOTERM_ROOT_DIR (dirname (status --current-filename))/../../

# Prepend the portable fish functions directory to the function search path.
# This is critical for tools like zoxide that need to wrap built-in functions.
set -p fish_function_path "$ISOTERM_ROOT_DIR/fish_runtime/share/fish/functions"

if status is-interactive
    starship init fish | source
    atuin init fish | source
    zoxide init fish | source
end

function on_exit --on-event fish_exit
    echo "Exiting isolated shell environment."
end