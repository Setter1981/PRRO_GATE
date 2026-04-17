using System;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class IniHGB
{
	private int IFN;

	private string FNI;

	private string strFilename;

	public string FileName => strFilename;

	[DllImport("kernel32.dll", CharSet = CharSet.Ansi, EntryPoint = "GetPrivateProfileStringA", ExactSpelling = true, SetLastError = true)]
	private static extern int GetPrivateProfileString([MarshalAs(UnmanagedType.VBByRefStr)] ref string lpApplicationName, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpKeyName, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpDefault, StringBuilder lpReturnedString, int nSize, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpFileName);

	[DllImport("kernel32.dll", CharSet = CharSet.Ansi, EntryPoint = "WritePrivateProfileStringA", ExactSpelling = true, SetLastError = true)]
	private static extern int WritePrivateProfileString([MarshalAs(UnmanagedType.VBByRefStr)] ref string lpApplicationName, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpKeyName, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpString, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpFileName);

	[DllImport("kernel32.dll", CharSet = CharSet.Ansi, EntryPoint = "GetPrivateProfileIntA", ExactSpelling = true, SetLastError = true)]
	private static extern int GetPrivateProfileInt([MarshalAs(UnmanagedType.VBByRefStr)] ref string lpApplicationName, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpKeyName, int nDefault, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpFileName);

	[DllImport("kernel32.dll", CharSet = CharSet.Ansi, EntryPoint = "WritePrivateProfileStringA", ExactSpelling = true, SetLastError = true)]
	private static extern int FlushPrivateProfileString(int lpApplicationName, int lpKeyName, int lpString, [MarshalAs(UnmanagedType.VBByRefStr)] ref string lpFileName);

	public IniHGB(string Filename)
	{
		IFN = 0;
		FNI = "";
		strFilename = Filename;
	}

	public string GetString(string Section, string Key, string Default = "")
	{
		string result = "";
		StringBuilder stringBuilder = new StringBuilder(256);
		int privateProfileString = GetPrivateProfileString(ref Section, ref Key, ref Default, stringBuilder, stringBuilder.Capacity, ref strFilename);
		if (privateProfileString > 0)
		{
			result = Strings.Left(stringBuilder.ToString(), privateProfileString);
		}
		return result;
	}

	public int GetInteger(string Section, string Key, int Default)
	{
		return GetPrivateProfileInt(ref Section, ref Key, Default, ref strFilename);
	}

	public void WriteString(string Section, string Key, string Value)
	{
		WritePrivateProfileString(ref Section, ref Key, ref Value, ref strFilename);
		Flush();
	}

	public void WriteInteger(string Section, string Key, int Value)
	{
		WriteString(Section, Key, Conversions.ToString(Value));
		Flush();
	}

	private void Flush()
	{
		FlushPrivateProfileString(0, 0, 0, ref strFilename);
	}

	public string StringGetFn(string Fn, string Name)
	{
		string text = GetString("Global", Name);
		if (Operators.CompareString(Name.Trim().ToLower(), "path", false) == 0)
		{
			if (Operators.CompareString(text.Trim().ToLower(), "default", false) == 0)
			{
				return All.MyDoc() + "\\WebCheck\\DB\\" + Fn.Trim() + ".db";
			}
			text = "";
		}
		if (Operators.CompareString(text.Trim(), "", false) != 0)
		{
			return text.Trim();
		}
		int num = IndexFn(Fn);
		if (num > 0)
		{
			return GetString("F" + num, Name);
		}
		return "";
	}

	public int IntegerGetFn(string Fn, string Name)
	{
		string @string = GetString("Global", Name);
		if (Operators.CompareString(@string.Trim(), "", false) == 0)
		{
			goto IL_003c;
		}
		int result;
		try
		{
			result = Conversions.ToInteger(@string);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
			goto IL_003c;
		}
		goto IL_0080;
		IL_0080:
		return result;
		IL_003c:
		int num = IndexFn(Fn);
		if (num > 0)
		{
			try
			{
				result = Conversions.ToInteger(GetString("F" + num, Name, "0"));
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				result = 0;
				ProjectData.ClearProjectError();
			}
		}
		else
		{
			result = 0;
		}
		goto IL_0080;
	}

	public void ClearIndexIni()
	{
		IFN = 0;
		FNI = "";
	}

	public void StringWriteFN(string Fn, string Name, string Val)
	{
		int num = IndexFn(Fn);
		if (num > 0)
		{
			WriteString("F" + num, Name, Val);
		}
	}

	public void IntigerWriteFN(string Fn, string Name, int Val)
	{
		WriteInteger("F" + IndexFn(Fn), Name, Val);
	}

	public void RenameFN(string OldName, string NewName)
	{
		int num = IndexFn(NewName);
		if (num <= 0)
		{
			ClearIndexIni();
			num = IndexFn(OldName);
			if (num > 0)
			{
				WriteString("Fns", num.ToString(), NewName);
			}
			ClearIndexIni();
		}
	}

	public void AddFn(string Fn)
	{
		ClearIndexIni();
		int num = IndexFn(Fn);
		if (num <= 0)
		{
			WriteString("Fns", IndexNewFn().ToString(), Fn);
		}
	}

	public void DelFn(string Fn)
	{
		ClearIndexIni();
		int num = IndexFn(Fn);
		if (num > 0)
		{
			WriteString("Fns", num.ToString(), "del");
		}
	}

	public int IndexFn(string Fn)
	{
		if ((IFN > 0) & (Operators.CompareString(FNI, Fn, false) == 0))
		{
			return IFN;
		}
		int num = 1;
		bool flag = false;
		while (!flag)
		{
			string @string = GetString("Fns", num.ToString());
			if (Operators.CompareString(@string, "", false) == 0)
			{
				return 0;
			}
			if (Operators.CompareString(Fn.Trim(), @string, false) == 0)
			{
				IFN = num;
				FNI = Fn;
				return num;
			}
			num = checked(num + 1);
		}
		ClearIndexIni();
		return 0;
	}

	private int IndexNewFn()
	{
		int num = 1;
		bool flag = false;
		while (!flag)
		{
			string @string = GetString("Fns", num.ToString());
			if (Operators.CompareString(@string.Trim(), "del", false) == 0)
			{
				return num;
			}
			if (@string.Trim().Length > 0 && !Versioned.IsNumeric((object)@string))
			{
				return num;
			}
			if (Operators.CompareString(@string, "", false) == 0)
			{
				return num;
			}
			num = checked(num + 1);
		}
		return 0;
	}

	public int IndexMaxFn()
	{
		int num = 1;
		bool flag = false;
		checked
		{
			while (!flag)
			{
				if (Operators.CompareString(GetString("Fns", num.ToString()), "", false) == 0)
				{
					return num - 1;
				}
				num++;
			}
			return 0;
		}
	}

	public string NameFn(int index)
	{
		return GetString("Fns", index.ToString());
	}
}
