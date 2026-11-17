//! Python file/scalar hooks over the SDK's single GBBQ cipher traversal.
use eltdx::offline::gbbq::python_legacy::{self, Cipher};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

struct PythonCipher<'py> {
    py: Python<'py>,
    content: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
}

fn slice<'py>(
    value: &Bound<'py, PyAny>,
    start: Bound<'py, PyAny>,
    end: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    value.get_item(
        value
            .py()
            .import("builtins")?
            .getattr("slice")?
            .call1((start, end))?,
    )
}

fn fixed_slice<'py>(
    value: &Bound<'py, PyAny>,
    start: usize,
    end: usize,
) -> PyResult<Bound<'py, PyAny>> {
    slice(
        value,
        start.into_pyobject(value.py())?.into_any(),
        end.into_pyobject(value.py())?.into_any(),
    )
}

fn unpack_word<'py>(
    unpack: Bound<'py, PyAny>,
    bytes: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut values = crate::offline_financial::unpack(unpack.call1(("<I", bytes))?, 1)?;
    Ok(values.remove(0))
}

fn global<'py>(module: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let builtins = module.py().import("builtins")?;
    builtins
        .getattr("getattr")?
        .call1((module, name, builtins.getattr(name)?))
}

impl<'py> PythonCipher<'py> {
    fn masked(
        &self,
        operation: &str,
        left: &Bound<'py, PyAny>,
        right: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let operator = self.py.import("operator")?;
        let value = operator.getattr(operation)?.call1((left, right))?;
        operator
            .getattr("and_")?
            .call1((value, self.module.getattr("_MASK32")?))
    }
}

impl<'py> Cipher for PythonCipher<'py> {
    type Word = Bound<'py, PyAny>;
    type Error = PyErr;
    fn key(&mut self, offset: usize) -> PyResult<Self::Word> {
        let unpack = self.module.getattr("struct")?.getattr("unpack")?;
        unpack_word(
            unpack,
            fixed_slice(&self.module.getattr("_BIN_KEYS")?, offset, offset + 4)?,
        )
    }
    fn input(&mut self, offset: usize) -> PyResult<Self::Word> {
        let unpack = self.module.getattr("struct")?.getattr("unpack")?;
        unpack_word(unpack, fixed_slice(&self.content, offset, offset + 4)?)
    }
    fn table(
        &mut self,
        word: &Self::Word,
        mask: Option<u32>,
        shift: u32,
        base: usize,
    ) -> PyResult<Self::Word> {
        let operator = self.py.import("operator")?;
        let mut index = if let Some(mask) = mask {
            operator.getattr("and_")?.call1((word, mask))?
        } else {
            word.clone()
        };
        if shift != 0 {
            index = operator.getattr("rshift")?.call1((index, shift))?;
        }
        // Capture unpack and keys before evaluating the original slice bounds.
        let unpack = self.module.getattr("struct")?.getattr("unpack")?;
        let keys = self.module.getattr("_BIN_KEYS")?;
        let multiply = operator.getattr("mul")?;
        let add = operator.getattr("add")?;
        let start = add.call1((multiply.call1((&index, 4))?, base))?;
        let end = add.call1((add.call1((multiply.call1((&index, 4))?, base))?, 4))?;
        unpack_word(unpack, slice(&keys, start, end)?)
    }
    fn xor_mask(&mut self, left: &Self::Word, right: &Self::Word) -> PyResult<Self::Word> {
        self.masked("xor", left, right)
    }
    fn add_mask(&mut self, left: &Self::Word, right: &Self::Word) -> PyResult<Self::Word> {
        self.masked("add", left, right)
    }
}

#[pyfunction]
pub fn offline_gbbq<'py>(
    py: Python<'py>,
    content: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let unpack = module.getattr("struct")?.getattr("unpack")?;
    let count = unpack_word(unpack, fixed_slice(&content, 0, 4)?)?;
    let result = PyList::empty(py);
    let range = global(&module, "range")?.call1((count,))?;
    let mut cipher = PythonCipher {
        py,
        content,
        module,
    };
    let mut offset = 4;
    for row in range.try_iter()? {
        row?;
        py.check_signals()?;
        let clear = global(&cipher.module, "bytearray")?.call0()?;
        for _ in 0..3 {
            let (left, right) = python_legacy::decrypt_block(&mut cipher, offset)?;
            let extend = clear.getattr("extend")?;
            let packed = cipher
                .module
                .getattr("struct")?
                .call_method1("pack", ("<II", left, right))?;
            extend.call1((packed,))?;
            offset += 8;
        }
        let extend = clear.getattr("extend")?;
        extend.call1((fixed_slice(&cipher.content, offset, offset + 5)?,))?;
        let values = crate::offline_financial::unpack(
            cipher
                .module
                .getattr("struct")?
                .call_method1("unpack", ("<B7sIBffff", clear))?,
            8,
        )?;
        let constructor = cipher.module.getattr("GbbqRecord")?;
        let args = PyDict::new(py);
        args.set_item("market", &values[0])?;
        args.set_item(
            "code",
            values[1]
                .call_method1("rstrip", (PyBytes::new(py, b"\0"),))?
                .call_method1("decode", ("utf-8",))?,
        )?;
        for (index, field) in [
            "datetime",
            "category",
            "hongli_panqianliutong",
            "peigujia_qianzongguben",
            "songgu_qianzongguben",
            "peigu_houzongguben",
        ]
        .iter()
        .enumerate()
        {
            args.set_item(field, &values[index + 2])?;
        }
        result.append(constructor.call((), Some(&args))?)?;
        offset += 5;
    }
    Ok(result.into_any().unbind())
}
