// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

// Note that this file gets minified by build.rs.
//
// If you add any more externally called functions, add `window` lines at the
// end of the file so the minifier leaves those functions intact.

let rttActive = false;

function appendTrace(data) {
  if (!data || data.length === 0) {
    return;
  }
  
  let ascii = '';
  for (let i = 0; i < data.length; i++) {
    const byte = parseInt(data[i], 16);
    if ((byte >= 32 && byte <= 126) || byte === 9 || byte === 10 || byte === 13) {
      ascii = ascii + String.fromCharCode(byte);
    } else {
      ascii = ascii + '.';
    }
  }
  
  const display = document.querySelector('.trace-display');
  if (display) {
    display.textContent = display.textContent + ascii;
    
    if (display.textContent.length > 4096) {
      display.textContent = display.textContent.slice(-3072);
    }
    
    display.scrollTop = display.scrollHeight;
  }
}

function fetchRttData() {
  if (!rttActive) {
    return;
  }
  
  fetch('/api/rtt/data')
    .then(response => response.json())
    .then(result => {
      appendTrace(result.data);
      
      if (rttActive) {
        setTimeout(fetchRttData, 100);
      }
    })
    .catch(error => {
      console.warn('RTT fetch error:', error);
      
      if (rttActive) {
        setTimeout(fetchRttData, 1000);
      }
    });
}

function startRtt() {
  const display = document.querySelector('.trace-display');
  if (display) {
    display.style.whiteSpace = 'pre';
    rttActive = true;
    fetchRttData();
  } else {
    setTimeout(startRtt, 50);
  }
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', startRtt);
} else {
  startRtt();
}

window.addEventListener('beforeunload', function() {
  rttActive = false;
});