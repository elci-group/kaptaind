// CommonJS exports — adapter should flag kind "cjs_export".
// Also exercises the ".cjs" extension in detect_files.
module.exports = function handler(req, res) {
  res.end("ok");
};

module.exports.helper = function helper() {};
