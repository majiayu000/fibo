# Fibo zsh integration. Fibo starts zsh with ZDOTDIR pointing here; this file
# puts the user's ZDOTDIR back first, so .zprofile, .zshrc and .zlogin load as usual.
if [[ -n "${MOORING_ZSH_ZDOTDIR+X}" ]]; then
    'builtin' 'export' ZDOTDIR="$MOORING_ZSH_ZDOTDIR"
    'builtin' 'unset' 'MOORING_ZSH_ZDOTDIR'
else
    'builtin' 'unset' 'ZDOTDIR'
fi

{
    'builtin' 'typeset' _fibo_file="${ZDOTDIR-$HOME}/.zshenv"
    [[ ! -r "$_fibo_file" ]] || 'builtin' 'source' '--' "$_fibo_file"
} always {
    if [[ -o 'interactive' ]]; then
        'builtin' 'typeset' _fibo_file="${MOORING_SHELL_INTEGRATION_DIR}/zsh/fibo-integration"
        [[ ! -r "$_fibo_file" ]] || 'builtin' 'source' '--' "$_fibo_file"
    fi
    'builtin' 'unset' '_fibo_file' 'MOORING_SHELL_INTEGRATION_DIR'
}
