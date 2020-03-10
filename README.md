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

![extension manifest successfully loaded](https://github.com/ajnewlands/chrome-ext/blob/master/images/chrome-extensions.PNG)

## Create the background script

The extensions manifest file referenced a script called [background.js](https://github.com/ajnewlands/chrome-ext/blob/master/background.js) so we had better create it before going much further.

Taking the script piece by piece, the first interesting bit is this:
```javascript
var port = chrome.runtime.connectNative('com.example.chrome_ext');
```

This is the directive which tells Chrome to launch an external program identified by an arbitrarily chosen identifier "com.example.rust_ext". Note the limitations on the characters used: alphanumerics, dots and underscores only.

The name will correspond to either a registry key (Windows) or configuration file name (Linux) which must be created according to these [instructions](https://developer.chrome.com/apps/nativeMessaging) under the header 'Native messaging host location'.


The next piece of interest handles the actual passing of messages to the external process.

```javascript
    port.onMessage.addListener(function(msg) {
        if (msg.hasOwnProperty("type")) {
            switch(msg.type) {
                case "go_to_url":
                    go_to_url(encodeURI(msg.url));
                    break;
                default:
                    console.log("Received unhandled message type: " + msg.type);
            }
        } else {
            console.log("Received" + JSON.stringify(msg));
        }
    });
```

The protocol defined by the browser is simple; each message is prefixed with four bytes (in native byte order) indicating the length of the message. The messages on the wire will be JSON, which the browser will transparently convert to/from objects. 

In this instance we have defined exactly one command that we support, which will be represented by the JSON object containing a "type" field set to "go_to_url" and a "url" field containing some arbitrary web destination. For the sake of brevity, some niceties such as validating that the url are actually present and valid have been skipped!

If we receive any other kind of message we simply log it to the console.

The go_to_url(someurl) function does exactly as the name implies:

```javascript
function go_to_url(url) {
    chrome.tabs.query({currentWindow: true, active: true}, function(tab) {
        chrome.tabs.update(tab.id, { url: url});
    });
}
```

The major caveat here is the explicit assumption that there is an active tab available to navigate with - this can fail if we are perusing a window that doesn't actually have tabs (the background scripting window for example). Again, some niceties that might be expected of production code are skipped in the favour of brevity.

Finally, we'll register listeners which will fire each time the browser starts to receive, or has finished receiving, a resource. 

```javascript
chrome.webRequest.onBeforeRequest.addListener(function (req) { 
    post_nav_msg("start", req.url);
}, { urls: ["*://*/*"]});

chrome.webRequest.onCompleted.addListener(function(req) {
    post_nav_msg("end", req.url);
}, { urls: ["*://*/*"]})

function post_nav_msg(mtype, url) {
    let msg = {
        type: mtype,
        url: url,
        time: performance.now()
    };
    port.postMessage(msg);
}
```

Each time these listeners trigger they will drop a message onto the bus indicating whether a request was started or completed, to what resource (URL) the request applies and a high precision time stamp provided by the browser. This is an example of the kind of telemetry data we can extract from the browser and perhaps store as a time series (say to automatically measure the performance of a site at the resource level after each commit and catch performance regressions).

## Create the native component

Or at least, we could do something with this telemetry if it were actually going somewhere; until we actually integrate the native component nothing much will in fact be happening.

First, we need [another JSON file](https://github.com/ajnewlands/chrome-ext/blob/master/com.example.chrome_ext.json) which essentially defines what program to run and which extensions are allowed to run it:
```json
{
    "name": "com.example.chrome_ext",
    "description": "Rust Chrome-ext",
    "path": ".\\target\\debug\\chrome-ext.exe",
    "type": "stdio",
    "allowed_origins": [
      "chrome-extension://poljkbdpmipggfnlkacnklgconikfaad/"
    ]
}
```

The important fields here are the name (which should correspond to the registry key on Windows or file name on Linux), the path to the executable (which on Windows can be a relative path, not on Linux) amd the allowed_origins, which must include the id Chrome allocated to our extension when we first loaded it.

Next and last, we need the actual executable itself!

This example will act as an intermediary between one browser and a RabbitMQ exchange. This will allow any number of analytics to subscribe to the generated data stream without needing to be aware of each other. It will be possible to trigger the browser to navigate to a given URL by dropping an appropriately formed message on the bus rather than needing to manually control the browser(s).

First up, we need to remember that Chrome expects all the messages to be framed with length delimiters (specifically, with a 4 byte, native endian, value) and passed over stdin and stdout. Whilst it's trivial to implement this naively, in fact we'll take advantage of the codecs included in the popular *tokio* async runtime library to create asynchronous wrappers around stdin and stdout that will take care of the message framing for us.

[browser.rs](https://github.com/ajnewlands/chrome-ext/blob/master/src/browser.rs)
```rust
use tokio::io::{Stdin, Stdout};
use tokio_util::codec::length_delimited::{Builder, LengthDelimitedCodec};
use tokio_util::codec::{FramedRead, FramedWrite};

/// Wraps an output stream in a length delimited (4 bytes, native endian) codec
pub fn writer(stdout: Stdout) -> FramedWrite<Stdout, LengthDelimitedCodec> {
    Builder::new().native_endian().new_write(stdout)
}

/// Wraps an input stream in a length delimited (4 bytes, native endian) codec
pub fn reader(stdin: Stdin) -> FramedRead<Stdin, LengthDelimitedCodec> {
    Builder::new().native_endian().new_read(stdin)
}
```

We'll keep those functions in out back pocket for a minute.

As described earlier, the chrome-ext executable will also maintain a RabbitMQ connection. The code is in [rabbit.rs](https://github.com/ajnewlands/chrome-ext/blob/master/src/rabbit.rs) and mostly generic. That being said, it's worth highlighting the input queue binding that dicates what messages this program will consume (or ignore):

```rust
        let id = Uuid::new_v4();
        let mut bindings = FieldTable::default();
        bindings.insert("service".into(), AMQPValue::LongString("chrome-ext".into()));
        bindings.insert("id".into(), AMQPValue::LongString(q.into()));
        bindings.insert("x-match".into(), AMQPValue::LongString("all".into()));

        let queue_opts = QueueDeclareOptions {
            durable: false,
            exclusive: true,
            auto_delete: true,
            nowait: false,
            passive: false,
        };
        let queue = chan
            .queue_declare(q, queue_opts, FieldTable::default())
            .await?;

        chan.queue_bind(q, ex, "", QueueBindOptions::default(), bindings)
            .await?;
```

Noting that this will be a 'headers' exchange in RabbitMQ parlance, the bindings ensure that this program will only accept messages that include at least two headers:
* a "service" header set to "chrome-ext"
* an "id" header set to a unique id generated by each instance of the extension at run time.
* each instance also generates a unique queue name (equivalent to the "id" number)

Note also that the queues are set to be exclusive and deleted when all consumers disconnect.

This combination has a number of ergonomic benefits:
* Any number of browser instances can connect to the one RabbitMQ exchange
* Instructions can be sent to any specific instance
* We can determine how many instances exist at any time by counting the queues (using the RabbitMQ management interface).

Also, outgoing messages will include the instance id in a header called "from-id". This enables telemetry receivers to listen to the output of multiple, concurrent, browser sessions and keep them separate.

Now that we have both the stdio and rabbit "halves" we can stitch them together into an [event loop](https://github.com/ajnewlands/chrome-ext/blob/fd58660a2284830d8c4b065cc61a4be00d3db84f/src/main.rs#L19)

```rust
    let mut fut1 = browser::reader(tokio::io::stdin()).into_future().fuse();
    let mut fut2 = rabbit.get_consumer("my tag").await?.into_future().fuse();

    let mut res: Result<(), String> = Ok(());
    while res == Ok(()) {
        res = select!(
            (msg, stream) = fut1 => match msg {
                Some(Ok(message)) => {
                    rabbit.publish(message.to_vec()).await?;
                    Ok(fut1 = stream.into_future().fuse())
                },
                Some(Err(e)) => Err(format!("Error reading from browser: {}",e)),
                None => Err(String::from("Browser connection was shut down at source")),
            },
            (msg, consumer) = fut2 => match msg {
                Some(Ok(delivery)) => {
                        browser_writer.send(Bytes::from(delivery.data)).await.map_err(|e| format!("Error sending data to browser: {}",e))?;
                        Ok(fut2 = consumer.into_future().fuse())
                },
                Some(Err(e)) => Err(format!("Rabbit error: {:?}", e)),
                None => Err(String::from("Rabbit connection was shut down at source")),
            },
        );
    }
```

Again, for the sake of brevity some niceties (notably message validation) are ommitted, resulting in a fairly brief loop.
As messages are received from the browser, they are pushed onto the bus with the addition of the identifying "from-id" header.
As messages are received from the bus (with the required headers, "service" and "id" in place) they are framed with the message length prefix and pushed through stdout to the browser.

## Putting it all together

To demonstrate our browser telemetry in action, we'll create a "go_to_url" message and send it to the bus using the RabbitMQ management interface, remembering to add the required "service" and "id" headers; the "id" value can be seen by examing the available queues (or their bindings).

![Creating a go_to_url message from the management interface](https://github.com/ajnewlands/chrome-ext/blob/master/images/rabbit-go_to_url.PNG)

The message content is an instance of the simple "go_to_url" instruction defined earlier in [background.js](https://github.com/ajnewlands/chrome-ext/blob/master/background.js), i.e.;

```rust
{
  "type": "go_to_url",
  "url": "https://www.rust-lang.org"
 }
```

Two things should happen; the active browser tab should navigate to https://www.rust-lang.org and a large number of messages of the following format should be dispatched to the chrome-ext exchange, representing each of the resources included in that web page:

```json
{
  "type": "end",
  "url": "https://www.rust-lang.org/static/fonts/woff2/FiraSans-Regular.latin.woff2",
  "time": 358798.73000000045
}
{
  "type": "start",
  "url": "https://www.rust-lang.org/static/images/site.webmanifest?v=ngJW8jGAmR",
  "time": 358991.2750000003
}
{
  "type": "end",
  "url": "https://www.rust-lang.org/static/images/site.webmanifest?v=ngJW8jGAmR",
  "time": 359076.0399999999
}
```
