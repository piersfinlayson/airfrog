// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

// Note that this file gets minifieed by build.rs.

document.getElementById('srUp').addEventListener('click', async () => {
  const button = document.getElementById('srUp');
  const status = document.getElementById('srStat');
  
  button.disabled = true;
  status.textContent = 'Updating settings...';
  status.className = 'status-message';
  
  try {
    const settings = {
      speed: document.getElementById('srSp').value,
      auto_connect: document.getElementById('srAc').checked,
      keepalive: document.getElementById('srKa').checked,
      refresh: document.getElementById('srRst').checked
    };
    
    const response = await fetch('/api/config/swd/runtime', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(settings)
    });
    
    if (!response.ok) {
      throw new Error(`Update failed: ${response.status}`);
    }
    
    status.textContent = 'Settings updated successfully!';
    status.className = 'status-message status ok';
    
    // Refresh page after 2 seconds to show updated values
    setTimeout(() => { window.location.reload(); }, 2000);
    
  } catch (error) {
    status.textContent = `Update failed: ${error.message}`;
    status.className = 'status-message status error';
  } finally {
    button.disabled = false;
  }
});

document.getElementById('stUp').addEventListener('click', async () => {
  const button = document.getElementById('stUp');
  const status = document.getElementById('stStat');
  
  button.disabled = true;
  status.textContent = 'Updating settings...';
  status.className = 'status-message';
  
  try {
    const settings = {
      speed: document.getElementById('stSp').value,
      auto_connect: document.getElementById('stAc').checked,
      keepalive: document.getElementById('stKa').checked,
      refresh: document.getElementById('stRst').checked
    };
    
    const response = await fetch('/api/config/swd/flash', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(settings)
    });
    
    if (!response.ok) {
      throw new Error(`Update failed: ${response.status}`);
    }
    
    status.textContent = 'Settings updated successfully!';
    status.className = 'status-message status ok';
    
    // Refresh page after 2 seconds to show updated values
    setTimeout(() => { window.location.reload(); }, 2000);
    
  } catch (error) {
    status.textContent = `Update failed: ${error.message}`;
    status.className = 'status-message status error';
  } finally {
    button.disabled = false;
  }
});

document.getElementById('ntUp').addEventListener('click', async () => {
  const button = document.getElementById('ntUp');
  const status = document.getElementById('ntStat');
  
  button.disabled = true;
  status.textContent = 'Updating network settings...';
  status.className = 'status-message';
  
  try {
    // Helper function to parse IP address
    const parseIp = (str) => {
      if (!str.trim()) return [255, 255, 255, 255]; // Empty = 0xFF fill
      const parts = str.split('.').map(n => parseInt(n, 10));
      return parts.length === 4 && parts.every(n => n >= 0 && n <= 255) ? parts : null;
    };
    
    // Validate and collect IP addresses
    const staIp = parseIp(document.getElementById('ntStaIp').value);
    const staNetmask = parseIp(document.getElementById('ntStaNetmask').value);
    const staGw = parseIp(document.getElementById('ntStaGw').value);
    const staDns0 = parseIp(document.getElementById('ntStaDns0').value);
    const staDns1 = parseIp(document.getElementById('ntStaDns1').value);
    const staNtp = parseIp(document.getElementById('ntStaNtp').value);
    const apIp = parseIp(document.getElementById('ntApIp').value);
    const apNetmask = parseIp(document.getElementById('ntApNetmask').value);
    
    // Check for invalid IP addresses
    if (!staIp || !staNetmask || !staGw || !staDns0 || !staDns1 || !apIp || !apNetmask) {
      throw new Error('Invalid IP address format');
    }
    
    const settings = {
      mode: document.getElementById('ntMode').value,
      sta_ssid: document.getElementById('ntStaSsid').value,
      sta_password: document.getElementById('ntStaPass').value,
      sta_v4_dhcp: document.getElementById('ntStaDhcp').checked,
      sta_v4_ip: staIp,
      sta_v4_netmask: staNetmask,
      sta_v4_gw: staGw,
      sta_v4_dns0: staDns0,
      sta_v4_dns1: staDns1,
      sta_v4_ntp: staNtp,
      ap_ssid: document.getElementById('ntApSsid').value,
      ap_password: document.getElementById('ntApPass').value,
      ap_v4_ip: apIp,
      ap_v4_netmask: apNetmask
    };
    
    const response = await fetch('/api/config/network/flash', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(settings)
    });
    
    if (!response.ok) {
      throw new Error(`Update failed: ${response.status}`);
    }
    
    status.textContent = 'Network settings updated successfully!';
    status.className = 'status-message status ok';
    
    // Refresh page after 2 seconds to show updated values
    setTimeout(() => { window.location.reload(); }, 2000);
    
  } catch (error) {
    status.textContent = `Update failed: ${error.message}`;
    status.className = 'status-message status error';
  } finally {
    button.disabled = false;
  }
});

function toggleStaticIp() {
  const dhcp = document.getElementById('ntStaDhcp').checked;
  const display = dhcp ? 'none' : 'table-row';
  document.getElementById('ntStaIpRow').style.display = display;
  document.getElementById('ntStaNetmaskRow').style.display = display;
  document.getElementById('ntStaGwRow').style.display = display;
  document.getElementById('ntStaDns0Row').style.display = display;
  document.getElementById('ntStaDns1Row').style.display = display;
  document.getElementById('ntStaNtpRow').style.display = display;
}

// Attach event listener for DHCP toggle
if (document.getElementById('ntStaDhcp')) {
  document.getElementById('ntStaDhcp').addEventListener('change', toggleStaticIp);
  toggleStaticIp(); // Set initial state
}