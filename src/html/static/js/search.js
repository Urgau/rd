const MAX_SEARCH_ELEMENTS = 25;

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
    rdSearchInputValue("");
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

    let inputValues = rdSearchInput.value.toLowerCase().split("::");    

    // Original from https://stackoverflow.com/a/34152244 : CC BY-SA 4.0
    function rdHasSubArray(master, sub) {
        return sub.every((i => v => i = rdIncludesIndexOf(master, v, i) + 1)(0));
    }

    function rdIncludesIndexOf(array, who, starti) {
      for (var i = starti; i < array.length; i++) {
        if (array[i].lower_case_name.includes(who) === true) {
          return (i);
        }
      }
      return (-1);
    }

    let matches = 0;
    for (const item of INDEX) {
      if (rdHasSubArray(item.components, inputValues) === true) {
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
        rdSearchItems.appendChild(block);

        matches += 1;
        if (matches == MAX_SEARCH_ELEMENTS) {
          break;
        }
      }
    }

    if (matches == 0) {
      var block = document.createElement("span");
      block.classList.add("ps-2");
      block.classList.add("pe-2");

      block.innerText = "Sorry, no result for your query.";

      rdSearchItems.appendChild(block);
    }

    var windowUrl = new URL(window.location);
    windowUrl.searchParams.set('search', rdSearchInput.value);
    rdHistoryReplace(windowUrl, "Result for " + rdSearchInput.value + " - Rust");
  } else {
    rdSearchItemsClear("none");

    var windowUrl = new URL(window.location);
    windowUrl.searchParams.delete('search');
    rdHistoryReplace(windowUrl, originalWindowTitle);
  }
}

function rdHistoryReplace(url, title) {
  window.history.replaceState({}, title, url);
  document.title = title;
}

function rdSearchInputValue(input) {
  rdSearchInput.value = input;
  rdSearchInputChange(null);
}

function rdSearchItemsClear(display) {
  rdSearchMenu.style.display = display;
  while (rdSearchItems.lastElementChild) {
    rdSearchItems.removeChild(rdSearchItems.lastElementChild);
  }
}

const originalWindowTitle = document.title;
const windowSearchParams = new URLSearchParams(window.location.search);
if (windowSearchParams.get("search") !== null) {
  rdSearchInputValue(windowSearchParams.get("search"));
  rdSearchInput.focus();
}