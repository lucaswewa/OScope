OScope, the server of a motorised brightfield microscope
========================================================

Installing it as a Windows service
----------------------------------

From an administrator's PowerShell, in this folder:

    powershell -ExecutionPolicy Bypass -File install.ps1

- The program goes in C:\Program Files\OScope.
- Its configuration and data go in C:\ProgramData\OScope.
- The service, oscope, starts on its own, and serves on http://127.0.0.1:8210/.

Removing it
-----------

    powershell -ExecutionPolicy Bypass -File uninstall.ps1

The configuration and the data stay in C:\ProgramData\OScope.

Running it without installing it
--------------------------------

    oscope-server.exe --config oscope.toml

It then keeps its data beside oscope.toml, and stops on Ctrl-C.

Licences
--------

OScope is under the MIT licence: LICENSE. The software it is built with, and
their licences, are in THIRD-PARTY-NOTICES.html.
