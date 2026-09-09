{defaultsFor}: {
  config,
  lib,
  pkgs,
  options,
  ...
}: let
  cfg = config.programs.scufris;
  system = pkgs.stdenv.hostPlatform.system;
  defaults = defaultsFor system;
  providerAvailable = lib.hasAttrByPath ["services" "ai-tools-api" "enable"] options;
  providerEnabled = providerAvailable && config.services.ai-tools-api.enable;
  providerBaseUrl =
    if providerAvailable
    then "http://${config.services.ai-tools-api.host}:${toString config.services.ai-tools-api.port}"
    else "http://127.0.0.1:10300";
  agentCfg = cfg.agent;
  briefingCfg = agentCfg.briefing;
  serviceCfg = cfg.service;
  remoteSurfaceCfg = serviceCfg.remoteSurface;
  managedApiCfg = cfg.aiToolsApi;
  desktopCfg = cfg.desktop;
  desktopApiCfg = desktopCfg.aiToolsApi;
  speechCfg = desktopCfg.speech;
  transcriptionCfg = desktopCfg.transcription;
  widgetCfg = desktopCfg.widgets;
  transcriptionEndpoint = "${desktopApiCfg.baseUrl}/v1/audio/transcriptions";
  speechEndpoint = "${desktopApiCfg.baseUrl}/v1/audio/speech";
  # The companion may only restart the backend service this module owns, so the
  # hook is generated here instead of accepting a command from the model or the
  # environment.
  backendRestart = pkgs.writeShellApplication {
    name = "scufris-restart-backend";
    runtimeInputs = [pkgs.systemd];
    text = ''
      exec systemctl --user restart ${lib.escapeShellArg "${serviceCfg.serviceName}.service"}
    '';
    meta.mainProgram = "scufris-restart-backend";
  };
  launcher = import ./launcher.nix {
    inherit pkgs;
    resources = defaults.resources;
    piPackage = agentCfg.piPackage;
    den = defaults.denPackage;
    briefing = defaults.briefingPackage;
    projectRoots = agentCfg.projectRoots;
  };
  # One unit pair for each profile, so a schedule is systemd's and the run
  # directory it collects into is that profile's own. The name is concrete
  # rather than a template instance: every profile carries its own OnCalendar,
  # which a template timer could not.
  briefingUnitName = name: "scufris-briefing-${name}";
  briefingRunner = name: profile:
    import ./briefing-unit.nix {
      inherit pkgs name profile;
      briefing = defaults.briefingPackage;
      jobs = defaults.jobsPackage;
      ctl = cfg.ctlPackage;
      pi = agentCfg.piPackage;
      projectRoots = agentCfg.projectRoots;
      inherit (briefingCfg) keepDays;
    };
  briefingProfileName = "^[A-Za-z0-9][A-Za-z0-9_-]*$";
  # The helper reads a TOML path and does not know Nix exists. This is one way
  # to produce that file: a typed option so a malformed entry fails the build
  # instead of costing a morning. Anyone not on NixOS writes the same file by
  # hand, and the reader cannot tell the difference.
  briefingFormat = pkgs.formats.toml {};
  briefingScalar = lib.types.oneOf [lib.types.bool lib.types.int lib.types.float lib.types.str];
  # A null says nothing was set and TOML has no null, so an unset key renders
  # as an absent one rather than as a value the reader would have to refuse.
  briefingSourceAttrs = source: lib.filterAttrs (name: value: name != "_module" && value != null && value != {}) source;
  briefingSources =
    lib.mapAttrs
    (_: profile: lib.mapAttrs (_: briefingSourceAttrs) profile)
    briefingCfg.sources;
  briefingConfigFile = briefingFormat.generate "scufris-config.toml" {briefings = briefingSources;};
  # The frontend owns the speaker, so the synthesiser is bound here and handed
  # to the companion. A deployment with no speech hands it nothing and the
  # companion stays silent.
  speak = import ./speak.nix {
    inherit pkgs;
    endpoint = speechEndpoint;
    model = speechCfg.model;
    voice = speechCfg.voice;
  };
in {
  imports = [
    (lib.mkRenamedOptionModule ["programs" "scufris" "piPackage"] ["programs" "scufris" "agent" "piPackage"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "projectRoots"] ["programs" "scufris" "agent" "projectRoots"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "finalPackage"] ["programs" "scufris" "agent" "package"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "service" "agentPackage"] ["programs" "scufris" "agent" "package"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "service" "agent"] ["programs" "scufris" "agent"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "voice"] ["programs" "scufris" "desktop" "speech"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "aiToolsApi" "manage"] ["programs" "scufris" "aiToolsApi" "enable"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "hotkey"] ["programs" "scufris" "desktop" "popupKey"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "cancelKey"] ["programs" "scufris" "desktop" "backgroundKey"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "stopKey"] ["programs" "scufris" "desktop" "abortKey"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "chatCommand"] ["programs" "scufris" "desktop" "terminalCommand"])
    # The journal is read in the widget backend now, so there is no command to
    # point at. Removed rather than renamed: a path that silently did nothing
    # would look like a working setting.
    (lib.mkRemovedOptionModule ["programs" "scufris" "desktop" "todayCommand"] "The journal widgets read the-den directly. Remove this option; set desktop.widgets.denPath if the journal is not where DEN_PATH says.")
    (lib.mkRemovedOptionModule ["programs" "scufris" "desktop" "widgets" "todayCommand"] "The journal widgets read the-den directly. Remove this option; set desktop.widgets.denPath if the journal is not where DEN_PATH says.")
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "denPath"] ["programs" "scufris" "desktop" "widgets" "denPath"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "macrosDatabase"] ["programs" "scufris" "desktop" "widgets" "macrosDatabase"])
    # Removed rather than renamed: the schedule is systemd's now and its type
    # is an attribute set of profiles, so a rename would carry a time of day
    # into an option that cannot hold one.
    (lib.mkRemovedOptionModule ["programs" "scufris" "agent" "briefing" "time"] "A briefing runs on its own systemd timer now. Set programs.scufris.agent.briefing.profiles instead, for example { morning.schedule = \"08:00\"; }, or {} for no scheduled briefing.")
  ];

  options.programs.scufris = {
    enable = lib.mkEnableOption "Scufris Pi launcher";

    ctlPackage = lib.mkOption {
      type = lib.types.package;
      default = defaults.ctlPackage;
      defaultText = lib.literalExpression "self.packages.\${system}.scufris-ctl";
      description = ''
        scufris-ctl package. Installed by whichever of the service and the
        companion is enabled, because a window manager binding and a terminal
        both reach Scufris by name and neither wants the other's package.
      '';
    };

    agent = {
      piPackage = lib.mkOption {
        type = lib.types.package;
        default = defaults.piPackage;
        defaultText = lib.literalExpression "inputs.llm-agents.packages.${system}.pi";
        description = "Pi package used by the default Scufris agent launcher.";
      };

      projectRoots = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = ["~/personal" "~/work" "~/third-party"];
        description = "Directories recursively searched for workflow projects.";
      };

      briefing = {
        profiles = lib.mkOption {
          type = lib.types.attrsOf (lib.types.submodule {
            options = {
              schedule = lib.mkOption {
                type = lib.types.str;
                example = "Mon..Fri 07:30";
                description = ''
                  systemd `OnCalendar` specification, in the host's own local
                  time. It is checked with `systemd-analyze calendar` while
                  this is built, so a schedule nobody can act on fails the
                  build rather than the morning. systemd reads none of
                  crontab's syntax: write `07:30`, `Mon *-*-* 09:00` or
                  `Mon..Fri 23:00`.
                '';
              };

              persistent = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = ''
                  Collect once at the next login when the machine was off at
                  the scheduled time. A briefing nobody was awake for is still
                  one that was never delivered. Set it false for a profile
                  that is only worth having on time.
                '';
              };

              deadline = lib.mkOption {
                type = lib.types.ints.positive;
                default = 1800;
                example = 3600;
                description = ''
                  Seconds the whole collection may take before it publishes
                  with whatever came back. The unit is given longer than this,
                  so a run is bounded by its own deadline and never killed
                  halfway by systemd.
                '';
              };

              sourceDeadline = lib.mkOption {
                type = lib.types.ints.positive;
                default = 900;
                example = 28800;
                description = ''
                  Seconds one source may take before it is recorded as failed.
                  A source is held to this or to whatever is left of
                  `deadline`, whichever is smaller, so raising this alone
                  changes nothing: a profile that wants a long source raises
                  both.

                  The default suits a briefing whose sources report what they
                  read. A profile whose sources do work, such as a nightly
                  review, is the reason this is not fixed.
                '';
              };

              parallel = lib.mkOption {
                type = lib.types.nullOr lib.types.ints.positive;
                default = null;
                example = 2;
                description = ''
                  How many sources may run at once. Null runs every source at
                  once, which is what a morning of cheap reports wants.

                  Set it for a profile whose sources are expensive, so that a
                  project declaring the profile later cannot quietly widen the
                  run. This counts sources and not what a source starts: what
                  a source spawns belongs to its harness and is not visible
                  here.
                '';
              };

              maxOffers = lib.mkOption {
                type = lib.types.ints.positive;
                default = 3;
                example = 8;
                description = ''
                  How many things one source may offer as worth doing next.
                  The number is stated in the prompt the source is given and
                  checked against the answer it sends back, so it is the same
                  number in both places.

                  Three suits a summary. A profile whose sources review
                  something and come back with a list of findings is the
                  reason this is not fixed.
                '';
              };

              maxBody = lib.mkOption {
                type = lib.types.ints.positive;
                default = 16384;
                example = 65536;
                description = ''
                  Characters of Markdown one source's body may carry. An
                  answer over the limit is rejected whole and asked again, so
                  this is a real bound on what a source can report and not a
                  trim.
                '';
              };
            };
          });
          default = {morning.schedule = "08:00";};
          example = lib.literalExpression ''
            {
              morning.schedule = "07:30";
              weekly = {
                schedule = "Mon *-*-* 09:00";
                deadline = 3600;
              };
            }
          '';
          description = ''
            Briefings that run on their own systemd user timer, one timer and
            one run directory for each. The attribute name is the profile:
            only projects declaring `[briefings.<name>]` in their own
            `.scufris.toml` contribute, so a schedule costs nothing until one
            does.

            When a briefing happens is declared here; what is in it is
            declared by each project. Nothing in this option says anything
            about a briefing's content.

            `{}` schedules none, and the tools still collect one when asked.
            Timers are systemd's, so nothing is scheduled off Linux.
          '';
        };

        sources = lib.mkOption {
          type = lib.types.attrsOf (lib.types.attrsOf (lib.types.submodule {
            freeformType = briefingFormat.type;
            options = {
              description = lib.mkOption {
                type = lib.types.str;
                example = "What Scufris did overnight.";
                description = ''
                  One short printable line saying what this source reports.
                '';
              };

              guidance = lib.mkOption {
                type = lib.types.lines;
                description = ''
                  What the source is asked. Nobody chooses a briefing source,
                  so this is required: it is the whole of what the source is
                  told to look at and report.
                '';
              };

              keywords = lib.mkOption {
                type = lib.types.attrsOf (lib.types.either briefingScalar (lib.types.listOf briefingScalar));
                default = {};
                example = lib.literalExpression ''{harness = "pi"; thinking = "medium";}'';
                description = ''
                  How the source is run, and anything else it should carry.
                  `harness`, `model` and `thinking` choose the adapter; every
                  value stays a flat scalar or a list of them, because the
                  source is given each one back verbatim.
                '';
              };

              root = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
                example = "/home/you/personal";
                description = ''
                  Where the source runs. Unset, it runs in the home directory,
                  which is what anything reading XDG state wants.
                '';
              };
            };
          }));
          default = {};
          example = lib.literalExpression ''
            {
              morning.jobs = {
                description = "What Scufris did overnight.";
                keywords = {harness = "pi"; thinking = "medium";};
                guidance = "Read what the helper measured and report it.";
              };
            }
          '';
          description = ''
            Briefing sources declared for the machine rather than for any
            project, written to `$XDG_CONFIG_HOME/scufris/config.toml`. The
            attribute path is `<profile>.<name>`: a source with no checkout to
            belong to, such as one reporting what the jobs helper measured.

            An ordinary source in every other way. It answers the same
            envelope, is held to the same deadlines, and is repaired and laid
            out the same. A project keeps declaring its own briefing in its own
            `.scufris.toml`, because a checkout has to work for someone whose
            machine has none of this.

            The file this renders is briefings-only, and the helper reads it as
            a path. Home Manager is one way to produce it; anyone else writes
            the same TOML by hand.
          '';
        };

        keepDays = lib.mkOption {
          type = lib.types.ints.positive;
          default = 30;
          example = 90;
          description = ''
            Days of briefings kept on disk. A day holds every profile that ran
            that day, so this is the same span of history whether one profile
            runs or several.

            It is not only disk. A source is told when this profile last ran,
            and that is read back through the days still kept, so a profile
            that runs less often than this keeps the days needs a larger
            number to be told anything true.
          '';
        };
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = launcher;
        defaultText = lib.literalExpression "the Scufris agent launcher rendered by this module";
        description = ''
          Interactive Scufris agent launcher installed as `scufris` and run by
          the service in RPC mode. Override it with another compatible harness.
        '';
      };
    };

    aiToolsApi = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = ''
          Manage the pinned complete ai-tools-api service. Leave false when the
          API is provided by services.ai-tools-api or outside Home Manager.
        '';
      };

      baseUrl = lib.mkOption {
        type = lib.types.strMatching "http://.*[^/]";
        default = providerBaseUrl;
        defaultText = lib.literalExpression "the enabled provider URL, otherwise http://127.0.0.1:10300";
        description = "Loopback base URL of the machine's shared inference API.";
      };
    };

    service = {
      enable = lib.mkEnableOption "the headless scufris-service background service";

      package = lib.mkOption {
        type = lib.types.package;
        default = defaults.servicePackage;
        defaultText = lib.literalExpression "self.packages.\${system}.scufris-service";
        description = "scufris-service package.";
      };

      sessionDirectory = lib.mkOption {
        type = lib.types.strMatching "/.*";
        default = "${config.xdg.dataHome}/scufris/sessions";
        defaultText = lib.literalExpression "\"\${config.xdg.dataHome}/scufris/sessions\"";
        description = ''
          Absolute directory where the service keeps the Pi model session. The
          service is its only owner.
        '';
      };

      conversationFile = lib.mkOption {
        type = lib.types.strMatching "/.*";
        default = "${config.xdg.dataHome}/scufris/conversation.json";
        defaultText = lib.literalExpression "\"\${config.xdg.dataHome}/scufris/conversation.json\"";
        description = ''
          Absolute file where the service keeps its bounded canonical surface
          replay. Its parent is private service-owned data.
        '';
      };

      serviceName = lib.mkOption {
        type = lib.types.str;
        default = "scufris-service";
        readOnly = true;
        description = "Stable systemd user service identity for the background service, without the unit suffix.";
      };

      remoteSurface = {
        enable = lib.mkEnableOption "the authenticated loopback WebSocket gateway for remote surfaces";

        port = lib.mkOption {
          type = lib.types.port;
          default = 10440;
          description = "Loopback gateway port consumed by the owned Tailscale Serve route.";
        };

        tokenFile = lib.mkOption {
          type = lib.types.nullOr (lib.types.strMatching "/.*");
          default = null;
          example = "/run/secrets/scufris-surface-token";
          description = ''
            Absolute path to a private file containing the remote surface
            bearer token. The file is read at gateway startup and is never
            copied into the Nix store.
          '';
        };

        serviceName = lib.mkOption {
          type = lib.types.str;
          default = "scufris-surface-gateway";
          readOnly = true;
          description = "Stable systemd user service identity for the remote surface gateway.";
        };

        tailscaleServiceName = lib.mkOption {
          type = lib.types.str;
          default = "scufris-tailscale-serve";
          readOnly = true;
          description = "Stable systemd user service identity for the private WSS endpoint.";
        };
      };
    };

    desktop = {
      enable = lib.mkEnableOption "the scufris-desktop voice pill and tray companion";

      package = lib.mkOption {
        type = lib.types.package;
        default = defaults.desktopPackage;
        defaultText = lib.literalExpression "self.packages.\${system}.scufris-desktop";
        description = "scufris-desktop companion package.";
      };

      aiToolsApi = {
        baseUrl = lib.mkOption {
          type = lib.types.strMatching "https?://.*[^/]";
          default = managedApiCfg.baseUrl;
          defaultText = lib.literalExpression "programs.scufris.aiToolsApi.baseUrl";
          description = "Base URL of the shared bounded speech inference API.";
        };
      };

      speech = {
        enable = lib.mkEnableOption "local speech synthesised through ai-tools-api";

        model = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9._-]+";
          default = "piper-1";
          description = "Speech model sent to ai-tools-api.";
        };

        voice = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9._-]+";
          default = "en_US-lessac-medium";
          description = "Speech voice sent to ai-tools-api.";
        };
      };

      transcription = {
        model = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9._-]+";
          default = "whisper-1";
          description = "Transcription model sent to ai-tools-api.";
        };

        language = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9_-]+";
          default = "auto";
          description = "Transcription language sent to ai-tools-api.";
        };
      };

      popupKey = lib.mkOption {
        type = lib.types.strMatching "[A-Za-z0-9+]+";
        default = "Super+D";
        description = "Key that shows the pill: tap for the workspace, hold to talk.";
      };

      backgroundKey = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "[A-Za-z0-9+]+|none");
        default = null;
        description = ''
          Key that puts the pill in the background and discards a current take.
          When null it is derived from the popup key's modifiers, so `Super+D`
          gives `Super+Escape`. Set it to `"none"` to leave the key to the
          desktop. The companion holds it only while the pill is visible.
        '';
      };

      abortKey = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "[A-Za-z0-9+]+|none");
        default = null;
        description = ''
          Key that aborts the current Scufris run. When null it is derived from
          the popup key's modifiers, so `Super+D` gives `Super+Delete`. Set it
          to `"none"` to leave the key to the desktop. The companion holds it
          only while the pill is visible.
        '';
      };

      terminalCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that opens a deployment-specific terminal view from the
          tray. Scufris ships no terminal session handoff protocol.
        '';
      };

      widgets = {
        denPath = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "/home/you/personal/the-den";
          description = ''
            Journal directory when it is not the default. A systemd user
            service does not inherit the login shell's DEN_PATH.
          '';
        };

        macrosDatabase = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "/home/you/.local/share/nvim/macros.csv";
          description = ''
            Food database when it is not the den's own Foods.csv. Unset, the
            journal answers first and Neovim's file only if the den has none.
          '';
        };
      };

      serviceName = lib.mkOption {
        type = lib.types.str;
        default = "scufris-desktop";
        readOnly = true;
        description = "Stable systemd user service identity for desktop consumers, without the unit suffix.";
      };

      restartCommand = lib.mkOption {
        type = lib.types.package;
        readOnly = true;
        description = "Generated hook that restarts only the Scufris backend service this module owns.";
      };
    };
  };

  config = lib.mkMerge [
    {
      programs.scufris.desktop.restartCommand = backendRestart;
    }
    (lib.mkIf cfg.enable {
      home.packages = [agentCfg.package];
    })
    # One file for the sources that belong to the machine and not to any
    # checkout. It is generated so a malformed entry fails the build rather
    # than the morning; the helper only ever sees a TOML path.
    (lib.mkIf (cfg.enable && briefingCfg.sources != {}) {
      assertions = [
        {
          assertion = lib.all (name: builtins.match briefingProfileName name != null) (lib.attrNames briefingCfg.sources);
          message = "programs.scufris.agent.briefing.sources profile names are letters, digits, dashes and underscores";
        }
        {
          assertion = lib.all (profile: lib.all (name: builtins.match briefingProfileName name != null) (lib.attrNames profile)) (lib.attrValues briefingCfg.sources);
          message = "programs.scufris.agent.briefing.sources names are letters, digits, dashes and underscores: the reader namespaces each one into a contribution file name";
        }
      ];

      xdg.configFile."scufris/config.toml".source = briefingConfigFile;
    })
    # The same numbers the timer exports, in a file every run can read.
    #
    # A profile's bounds only ever reached a run the timer started: the unit
    # exports them into its own environment. A briefing asked for by hand - the
    # `scufris_briefing_run` tool, or a shell - got the code defaults instead
    # and said nothing, so a night profile that allows eight hours a source was
    # silently cut at fifteen minutes. The environment still wins where it is
    # set, so a run asking for a number by hand still gets it.
    (lib.mkIf (cfg.enable && briefingCfg.profiles != {}) {
      xdg.configFile."scufris/briefing-profiles.json".text = builtins.toJSON (lib.mapAttrs (_: profile: {
          inherit (profile) deadline parallel;
          # The helper's names, which are the environment variables' names. The
          # option names are Home Manager's convention and stop here.
          source_deadline = profile.sourceDeadline;
          max_offers = profile.maxOffers;
          max_body = profile.maxBody;
          keep_days = briefingCfg.keepDays;
        })
        briefingCfg.profiles);
    })
    # A briefing is collected out of process and delivered over the control
    # socket, so the schedule needs neither a session nor an agent. The user
    # manager runs from login to logout, which is where Scufris lives, and
    # `Persistent=true` catches up a briefing the machine was off for.
    (lib.mkIf (cfg.enable && pkgs.stdenv.hostPlatform.isLinux && briefingCfg.profiles != {}) {
      assertions = [
        {
          assertion = lib.all (name: builtins.match briefingProfileName name != null) (lib.attrNames briefingCfg.profiles);
          message = "programs.scufris.agent.briefing.profiles names are letters, digits, dashes and underscores: they are run directory names and a command line argument";
        }
      ];

      systemd.user.services = lib.mapAttrs' (name: profile:
        lib.nameValuePair (briefingUnitName name) {
          Unit.Description = "Scufris ${name} briefing";
          Service = {
            Type = "oneshot";
            ExecStart = lib.getExe (briefingRunner name profile);
            # Above the deadline the collection holds itself to, so a run
            # still asking its sources is never killed halfway. What it has
            # gathered by then is published either way.
            TimeoutStartSec = profile.deadline + 300;
            WorkingDirectory = "%h";
          };
        })
      briefingCfg.profiles;

      systemd.user.timers = lib.mapAttrs' (name: profile:
        lib.nameValuePair (briefingUnitName name) {
          Unit.Description = "Scufris ${name} briefing schedule";
          Timer = {
            OnCalendar = profile.schedule;
            # One catch-up, by the clock systemd keeps, instead of a rule
            # written here about what a late session owes the day.
            Persistent = profile.persistent;
          };
          Install.WantedBy = ["timers.target"];
        })
      briefingCfg.profiles;
    })
    (lib.mkIf (cfg.enable && managedApiCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.aiToolsApi" pkgs lib.platforms.linux)
        {
          assertion = !providerEnabled;
          message = "programs.scufris.aiToolsApi.enable conflicts with an enabled services.ai-tools-api provider";
        }
        {
          assertion = managedApiCfg.baseUrl == "http://127.0.0.1:10300";
          message = "the managed Scufris ai-tools-api uses http://127.0.0.1:10300; disable management for another base URL";
        }
      ];
    })
    (lib.mkIf (cfg.enable && speechCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.desktop.speech" pkgs lib.platforms.linux)
      ];
    })
    (lib.mkIf (cfg.enable && serviceCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.service" pkgs lib.platforms.linux)
      ];

      home.packages = [serviceCfg.package cfg.ctlPackage];

      systemd.user.services.${serviceCfg.serviceName} = {
        Unit = {
          # No graphical session, and nothing ordered after one. The
          # service is the half that keeps the conversation whether or not
          # anything is on screen, and a terminal over ssh reaches it with
          # scufris-ctl.
          Description = "Scufris background service";
        };
        Service = {
          Type = "simple";
          ExecStart = lib.getExe serviceCfg.package;
          # Nothing about speech. The agent shapes every answer as one
          # prose paragraph whatever is listening, and whether a sound is
          # made is the companion's, which is where the speaker is.
          Environment = [
            "SCUFRIS_SERVICE_AGENT=${lib.getExe agentCfg.package}"
            "SCUFRIS_SERVICE_SESSION_DIR=${serviceCfg.sessionDirectory}"
            "SCUFRIS_SERVICE_CONVERSATION_FILE=${serviceCfg.conversationFile}"
          ];
          # The service restarts its own agent, so it going down is a fault of
          # the service itself and the conversation is on disk either way.
          Restart = "on-failure";
          RestartSec = 3;
          # No `RuntimeDirectory`. It made `%t/scufris-service`, which nothing
          # uses: the sockets live in `%t/scufris`, at 0700, created by the
          # code that binds them. Pointing the option at that directory would
          # be worse than leaving it - systemd would relax it to 0755 and
          # delete it when this unit stops, taking the companion's and the
          # gateway's sockets with it.
          WorkingDirectory = "%h";
        };
        Install.WantedBy = ["default.target"];
      };
    })
    (lib.mkIf (cfg.enable && remoteSurfaceCfg.enable) {
      assertions = [
        {
          assertion = serviceCfg.enable;
          message = "programs.scufris.service.remoteSurface.enable requires programs.scufris.service.enable";
        }
        {
          assertion = remoteSurfaceCfg.tokenFile != null;
          message = "programs.scufris.service.remoteSurface.enable requires programs.scufris.service.remoteSurface.tokenFile";
        }
      ];

      home.packages = [pkgs.tailscale];

      systemd.user.services = {
        ${serviceCfg.serviceName}.Unit.Wants = ["${remoteSurfaceCfg.serviceName}.service"];

        ${remoteSurfaceCfg.serviceName} = {
          Unit = {
            Description = "Scufris authenticated remote surface gateway";
            After = ["${serviceCfg.serviceName}.service"];
            Requires = ["${serviceCfg.serviceName}.service"];
          };
          Service = {
            Type = "simple";
            ExecStart = "${lib.getExe' serviceCfg.package "scufris-surface-gateway"} --listen 127.0.0.1:${toString remoteSurfaceCfg.port} --token-file ${lib.escapeShellArg (
              if remoteSurfaceCfg.tokenFile == null
              then "/missing-scufris-surface-token"
              else remoteSurfaceCfg.tokenFile
            )} --ai-tools-api ${lib.escapeShellArg managedApiCfg.baseUrl}";
            Restart = "on-failure";
            RestartSec = 3;
          };
          Install.WantedBy = ["default.target"];
        };

        ${remoteSurfaceCfg.tailscaleServiceName} = {
          Unit = {
            Description = "Scufris private WSS endpoint";
            After = ["${remoteSurfaceCfg.serviceName}.service"];
            Wants = ["${remoteSurfaceCfg.serviceName}.service"];
          };
          Service = {
            Type = "oneshot";
            RemainAfterExit = true;
            ExecStart = "${lib.getExe pkgs.tailscale} serve --bg --yes --set-path / http://127.0.0.1:${toString remoteSurfaceCfg.port}";
            ExecStop = "-${lib.getExe pkgs.tailscale} serve --https=443 --set-path / off";
            Restart = "on-failure";
            RestartSec = 3;
          };
          Install.WantedBy = ["default.target"];
        };
      };
    })
    (lib.mkIf (cfg.enable && desktopCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.desktop" pkgs lib.platforms.linux)
        {
          assertion = serviceCfg.enable;
          message = "programs.scufris.desktop.enable requires programs.scufris.service.enable, because the companion is a client of the service that owns the conversation";
        }
        {
          assertion = !managedApiCfg.enable || desktopApiCfg.baseUrl == "http://127.0.0.1:10300";
          message = "the managed Scufris ai-tools-api fallback uses http://127.0.0.1:10300; set programs.scufris.aiToolsApi.enable=false for another base URL";
        }
      ];

      home.packages = [desktopCfg.package cfg.ctlPackage];

      systemd.user.services.${desktopCfg.serviceName} = {
        Unit = {
          Description = "Scufris voice pill and tray companion";
          PartOf = ["graphical-session.target"];
          After = ["graphical-session.target"];
        };
        Service = {
          Type = "simple";
          ExecStart = lib.getExe desktopCfg.package;
          Environment =
            [
              "SCUFRIS_STT_ENDPOINT=${transcriptionEndpoint}"
              "SCUFRIS_STT_MODEL=${transcriptionCfg.model}"
              "SCUFRIS_STT_LANGUAGE=${transcriptionCfg.language}"
              "SCUFRIS_DESKTOP_HOTKEY=${desktopCfg.popupKey}"
              "SCUFRIS_DESKTOP_RESTART_COMMAND=${lib.getExe backendRestart}"
            ]
            ++ lib.optional (desktopCfg.backgroundKey != null)
            "SCUFRIS_DESKTOP_CANCEL_KEY=${desktopCfg.backgroundKey}"
            ++ lib.optional (desktopCfg.abortKey != null)
            "SCUFRIS_DESKTOP_STOP_KEY=${desktopCfg.abortKey}"
            ++ lib.optional (desktopCfg.terminalCommand != null)
            "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe desktopCfg.terminalCommand}"
            ++ lib.optional (widgetCfg.denPath != null)
            "DEN_PATH=${widgetCfg.denPath}"
            ++ lib.optional (widgetCfg.macrosDatabase != null)
            "MACROS_DATABASE=${widgetCfg.macrosDatabase}"
            ++ lib.optional speechCfg.enable
            "SCUFRIS_DESKTOP_SPEAK_COMMAND=${lib.getExe speak}";
          # The companion must survive its own faults; a backend crash is
          # reported in the tray instead of taking the companion down.
          Restart = "on-failure";
          RestartSec = 3;
          # Holds the accepted transcript that has not been acknowledged, so a
          # companion restart resumes with it instead of losing it.
          StateDirectory = desktopCfg.serviceName;
          WorkingDirectory = "%h";
        };
        Install.WantedBy = ["graphical-session.target"];
      };
    })
    (lib.mkIf (cfg.enable && managedApiCfg.enable) {
      systemd.user.services.scufris-ai-tools-api = {
        Unit = {
          Description = "Scufris fallback AI tools API";
          After = ["network.target"];
        };
        Service = {
          Type = "simple";
          ExecStart = lib.getExe defaults.aiToolsApiPackage;
          Restart = "on-failure";
          RestartSec = 5;
          RuntimeDirectory = "scufris-ai-tools-api";
          WorkingDirectory = "%t/scufris-ai-tools-api";
          NoNewPrivileges = true;
          PrivateTmp = true;
          ProtectSystem = "strict";
          ProtectHome = "tmpfs";
          UMask = "0077";
          TimeoutStopSec = 10;
        };
        Install.WantedBy = ["default.target"];
      };
    })
  ];
}
