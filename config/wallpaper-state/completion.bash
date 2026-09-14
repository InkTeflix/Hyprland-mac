# ============================================================
# hypr-mac wallpaper completion for bash
# ============================================================


# ------------------------------------------------------------
# wpp command
# ------------------------------------------------------------

wpp()
{
    "$HOME/.config/hypr-mac/wallpaper" "$@"
}


# ------------------------------------------------------------
# Completion
# ------------------------------------------------------------

_wpp_completion()
{
    local cur
    local commands
    local modes


    cur="${COMP_WORDS[COMP_CWORD]}"


    commands="
        start
        stop
        status
        current
        reload
        next
        previous
        set
        mode
        setup
        setup-zsh
        setup-bash
    "


    modes="
        manual
        periodic
        family-time
        rotate-time
    "


    # ========================================================
    # wpp <TAB>
    # ========================================================

    if (( COMP_CWORD == 1 )); then

        COMPREPLY=(
            $(compgen -W "$commands" -- "$cur")
        )

        return

    fi


    # ========================================================
    # wpp set <TAB>
    # ========================================================

    if [[ "${COMP_WORDS[1]}" == "set" ]]; then

        if (( COMP_CWORD == 2 )); then

            local files


            files="$(
                find \
                    "$HOME/.config/hypr-mac/wpp" \
                    -type f \
                    \( \
                        -iname '*.png' \
                        -o -iname '*.jpg' \
                        -o -iname '*.jpeg' \
                        -o -iname '*.webp' \
                        -o -iname '*.gif' \
                        -o -iname '*.bmp' \
                    \) \
                    -printf '%P\n' \
                    2>/dev/null
            )"


            COMPREPLY=(
                $(compgen -W "$files" -- "$cur")
            )

            return

        fi

    fi


    # ========================================================
    # wpp mode <TAB>
    # ========================================================

    if [[ "${COMP_WORDS[1]}" == "mode" ]]; then

        if (( COMP_CWORD == 2 )); then

            COMPREPLY=(
                $(compgen -W "$modes" -- "$cur")
            )

            return

        fi

    fi
}


# ------------------------------------------------------------
# Register completion
# ------------------------------------------------------------

complete -F _wpp_completion wpp
