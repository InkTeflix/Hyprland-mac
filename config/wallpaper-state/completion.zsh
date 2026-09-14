# ============================================================
# hypr-mac wallpaper completion for zsh
# ============================================================


# ------------------------------------------------------------
# Initialize zsh completion if necessary
# ------------------------------------------------------------

if (( ! $+functions[compdef] )); then
    autoload -Uz compinit
    compinit
fi


# ------------------------------------------------------------
# wpp command
# ------------------------------------------------------------

wpp()
{
    "$HOME/.config/hypr-mac/wallpaper" "$@"
}


# ------------------------------------------------------------
# Completion function
# ------------------------------------------------------------

_wpp()
{
    local -a commands
    local -a modes


    commands=(
        'start:Start the background wallpaper controller'
        'stop:Stop the background wallpaper controller'
        'status:Show controller state'
        'current:Show the wallpaper currently reported by awww'
        'reload:Rescan the wallpaper directory'
        'next:Select the next wallpaper'
        'previous:Select the previous wallpaper'
        'set:Set a specific wallpaper'
        'mode:Change automatic wallpaper mode'
        'setup:Check/setup shell integration'
        'setup-zsh:Check zsh integration'
        'setup-bash:Check bash integration'
    )


    modes=(
        'manual:Disable automatic wallpaper changes'
        'periodic:Random wallpaper every N minutes'
        'family-time:Stay in one family and follow time of day'
        'rotate-time:Rotate families and follow time of day'
    )


    case "$words[2]" in

        # ----------------------------------------------------
        # wallpaper selection
        # ----------------------------------------------------

        set)

            if (( CURRENT == 3 )); then

                _files \
                    -W "$HOME/.config/hypr-mac/wpp" \
                    -g '*.{png,jpg,jpeg,webp,gif,bmp}'

            fi

            ;;


        # ----------------------------------------------------
        # modes
        # ----------------------------------------------------

        mode)

            if (( CURRENT == 3 )); then

                _describe \
                    'mode' \
                    modes


            elif (( CURRENT == 4 )); then

                case "$words[3]" in

                    periodic|rotate-time)

                        _message \
                            'minutes'

                        ;;

                esac

            fi

            ;;


        # ----------------------------------------------------
        # default
        # ----------------------------------------------------

        *)

            if (( CURRENT == 2 )); then

                _describe \
                    'command' \
                    commands

            fi

            ;;

    esac
}


# ------------------------------------------------------------
# Register completion
# ------------------------------------------------------------

compdef _wpp wpp
