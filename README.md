# Exploring Chrome extensibility via native messaging and Rust.

## Overview
This repository contains a simple exploration of the native messaging capability of Chrome (and Firefox/Chromium based Edge) extensions.

This enables the creation of a Chrome extension which will launch native (to the underlying operating system) processes in the background. The extension can pass messages to, and receive them from, this background process. 

Amongst other things, this enables us to instrument the browser in various ways (e.g. to capture resource load time statistics) and pass the data gathered to external processes for storage and analysis. Equally, this allows external processes to trigger various internal browser APIs (for example, to change the proxy configuration or navigate to different URLs).

The example extension here will allow us to remotely access web navigation functionality for the active browser tab and receive resource load time telemetry back. Communications will be via a (RabbitMQ) message bus which allows for easy sharing of the telemetry data between multiple consumers (e.g. analytics implemented as stand alone microservices).

## Getting Started - Register an Extension with Chrome

To create a brand new Chrome extension, the first thing to do is provide a manifest file in JSON format. 

[manifest.json](https://github.com/ajnewlands/chrome-ext/blob/master/manifest.json)
```json
{
    "name": "Rust native messaging example",
    "version": "1.0",
    "description": "Trigger Chrome APIs from Rust",
    "permissions": ["tabs", "nativeMessaging", "webRequest", "*://*/*"],
    "background": {
      "scripts": ["background.js"],
      "persistent": true
    },
    "manifest_version": 2
  }
```

The import pieces are the "permissions" and "background" elements.

The given permissions given enable various API functions (as defined at https://developer.chrome.com/extensions/api_index) to be called from our extensions. The URL filter ("*://*/*") indicates which URLs are accessible by the webRequest API functions, i.e. all of them in this instance.

The background element indicates that our extension will load a single piece of Javascript, [background.js](https://github.com/ajnewlands/chrome-ext/blob/master/background.js). Setting "persistent" to true ensures that the script will be loaded when the browser starts and kept running indefinitely thereafter in the background.

At this point it's possible to load the extension by going to chrome://extensions and, if in developer mode, selecting 'load unpacked' and navigating to the folder containing the manifest file.

If successful, our extension will appear as a new panel in the list of extensions. Aside from a name, version number and description, you will note that an ID has been generated. We'll need this later to configure native messaging.

## Create the background script

The extensions manifest file referenced a script called [background.js](https://github.com/ajnewlands/chrome-ext/blob/master/background.js) so we had better create it before going much further.

Taking the script piece by piece, the first interesting bit is this:
```javascript
var port = chrome.runtime.connectNative('com.example.rust_ext');
```

This is the directive which tells Chrome to launch an external program identified by an arbitrarily chosen identifier "com.example.rust_ext". Note the limitations on the characters used: alphanumerics, dots and underscores only.

The name will correspond to either a registry key (Windows) or configuration file name (Linux) which must be created according to these [instructions](https://developer.chrome.com/apps/nativeMessaging) under the header 'Native messaging host location'.
