// See allowed characters at https://developer.chrome.com/apps/nativeMessaging
var port = chrome.runtime.connectNative('com.example.chrome_ext');

function go_to_url(url) {
    console.log("got a navigate instruction");

    chrome.tabs.query({currentWindow: true, active: true}, function(tab) {
        chrome.tabs.update(tab.id, { url: url});
    });
}

function post_nav_msg(mtype, url) {
    let msg = {
        type: mtype,
        url: url,
        time: performance.now()
    };
    port.postMessage(msg);
}

function event_loop() {
    console.log("Extension loaded");

    chrome.webRequest.onBeforeRequest.addListener(function (req) { 
        post_nav_msg("start", req.url);
    }, { urls: ["*://*/*"]});

    chrome.webRequest.onCompleted.addListener(function(req) {
        post_nav_msg("end", req.url);
    }, { urls: ["*://*/*"]})

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

    port.onDisconnect.addListener(function() {
        console.log("Disconnected");
    });
}

// Called when Chrome starts up this extension
chrome.runtime.onStartup.addListener(event_loop());