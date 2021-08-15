const rdSearchMenu = document.getElementById('rd-search-menu');
const rdSearchInput = document.getElementById('rd-search-input');
const rdSearchItems = document.getElementById('rd-search-items');
const rdSearchForm = document.getElementById('rd-search-form');

rdSearchInput.addEventListener('input', rdSearchInputChange);
rdSearchForm.addEventListener('focusout', rdSearchFormFocusOut);
rdSearchForm.addEventListener('focusin', rdSearchFormFocusIn);

document.onkeyup = (e) => {
  if (e.ctrlKey && e.key === "/") {
    rdSearchInput.focus();
  } else if (e.key == "Escape" && rdSearchInputFocused === true) {
    rdSearchItemsClear("none");
    rdSearchInput.value = "";
  }
};

var rdSearchInputFocused = false;
function rdSearchFormFocusOut(e) {
  rdSearchInputFocused = false;
}

function rdSearchFormFocusIn(e) {
  rdSearchInputFocused = true;
}

function rdSearchInputChange(e) {
  if (rdSearchInput.value !== "") {
    rdSearchItemsClear("block");

    let inputValue = rdSearchInput.value.toLowerCase();

    let matches = 0;
    for (const item of INDEX) {
      if (item.components.some((e) => e.lower_case_name.includes(inputValue)) === true) {

        var block = document.createElement("a");
        block.classList.add("rd-search-item");
        
        for (const [index, c] of item.components.entries()) {
          var span = document.createElement("span");
          span.classList.add(c.kind);
          span.innerText = c.name;

          block.appendChild(span);
          if (index + 1 != item.components.length) {
            block.innerText += "::";
          }
        }

        var mod_name = item.filepath.split('/')[0];
        var v = window.location.pathname.split('/');

        var before = "";
        for (var i = v.length - 1; i >= 0; i--) {
          if (v[i] == mod_name) {
            break;
          }
          before += "../";
        }

        block.href = before + item.filepath;

        matches += 1;
        rdSearchItems.appendChild(block);
      }
    }

    if (matches == 0) {
      var block = document.createElement("span");
      block.classList.add("ps-2");
      block.classList.add("pe-2");

      block.innerText = "Sorry, no result for your query.";

      rdSearchItems.appendChild(block);
    }
  } else {
    rdSearchItemsClear("none");
  }
}

function rdSearchItemsClear(display) {
  rdSearchMenu.style.display = display;
  while (rdSearchItems.lastElementChild) {
    rdSearchItems.removeChild(rdSearchItems.lastElementChild);
  }
}