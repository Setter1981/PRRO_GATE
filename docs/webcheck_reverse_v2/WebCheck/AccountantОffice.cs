using System;
using System.Collections.Generic;
using System.Dynamic;
using System.IO;
using System.Net;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Newtonsoft.Json;

namespace WebCheck;

internal class AccountantОffice
{
	internal int InfaC;

	internal TypNumFiscal[] Infa;

	internal TypEntity InfaTin;

	public AccountantОffice()
	{
		InfaC = 0;
		Infa = new TypNumFiscal[checked(InfaC + 1)];
	}

	internal string NameKeyINI(string eP, int eN, int eL = 5)
	{
		eP = eP.ToUpper();
		if (eP.Length > 2)
		{
			eP = eP.Substring(0, 2);
		}
		string text = eN.ToString();
		string text2 = "";
		checked
		{
			int num = eL - text.Length;
			for (int i = 0; i <= num; i++)
			{
				text2 += "0";
			}
			return eP + text2 + text;
		}
	}

	internal int CounterKeysTin(string eT)
	{
		eT = eT.Trim();
		int num = 0;
		do
		{
			if (Operators.CompareString(All.ArS.StringGetFn(eT, NameKeyINI("Fn", Conversions.ToInteger(num.ToString()))).Trim(), "", TextCompare: false) == 0)
			{
				return num;
			}
			num = checked(num + 1);
		}
		while (num <= 9999);
		return -1;
	}

	internal int IndexKeysTin(string eT, string eF)
	{
		eF = eF.Trim();
		if (Operators.CompareString(eF, "", TextCompare: false) == 0)
		{
			return -1;
		}
		int num = 9999;
		for (int i = 0; i <= num; i = checked(i + 1))
		{
			string left = All.ArS.StringGetFn(eT, NameKeyINI("Fn", Conversions.ToInteger(i.ToString()))).Trim();
			if (Operators.CompareString(left, "", TextCompare: false) == 0)
			{
				return -1;
			}
			if (Operators.CompareString(left, eF, TextCompare: false) == 0)
			{
				return i;
			}
		}
		return -1;
	}

	internal bool Dereban(string JS)
	{
		checked
		{
			bool result;
			try
			{
				string[] array = JS.Split('{');
				int num = array.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					array[i] = Strings.Replace(array[i], "{", "");
					array[i] = Strings.Replace(array[i], "}", "");
					array[i] = Strings.Replace(array[i], "[", "\"WebCheck\"");
					array[i] = Strings.Replace(array[i], "],", "H*G*B");
					array[i] = Strings.Replace(array[i], "]", "");
				}
				InfaTin.TIN = "";
				InfaTin.OrgName = "";
				InfaTin.Address = "";
				InfaTin.IPN = "";
				InfaTin.NameG = "";
				InfaC = 0;
				Infa = new TypNumFiscal[InfaC + 1];
				int num2 = array.Length - 1;
				for (int i = 2; i <= num2; i++)
				{
					if (Operators.CompareString(KeyJ(array[i], "Entity"), "", TextCompare: false) != 0)
					{
						InfaTin.OrgName = KeyJ(array[i], "OrgName");
						InfaTin.Address = KeyJ(array[i], "Address");
						InfaTin.IPN = KeyJ(array[i], "Ipn");
						InfaTin.TIN = KeyJ(array[i], "Tin");
						InfaTin.NameG = KeyJ(array[i], "Name");
					}
					else if (Operators.CompareString(KeyJ(array[i], "Closed", fn: true).ToLower(), "false", TextCompare: false) == 0)
					{
						Infa[InfaC].OrgName = InfaTin.OrgName;
						Infa[InfaC].Address = InfaTin.Address;
						Infa[InfaC].IPN = InfaTin.IPN;
						Infa[InfaC].TIN = InfaTin.TIN;
						Infa[InfaC].NumFiscal = KeyJ(array[i], "NumFiscal", fn: true);
						Infa[InfaC].Name = KeyJ(array[i], "Name", fn: true);
						InfaC++;
						ref TypNumFiscal[] infa = ref Infa;
						infa = (TypNumFiscal[])Utils.CopyArray(infa, new TypNumFiscal[InfaC + 1]);
					}
					Application.DoEvents();
				}
				result = true;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private string KeyJ(string strJS, string Key, bool fn = false)
	{
		string result;
		try
		{
			if (!fn)
			{
				strJS = "{" + strJS.Trim() + "}";
			}
			else
			{
				strJS = "{" + strJS.Replace("e,", "e").Trim() + "}";
				strJS = Strings.Replace(strJS, "H*G*B", ",");
			}
			result = ((IDictionary<string, object>)JsonConvert.DeserializeObject<ExpandoObject>(strJS))[Key.Trim()].ToString();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool FileForSend(string fileN)
	{
		bool result;
		try
		{
			StreamWriter streamWriter = new StreamWriter(fileN);
			streamWriter.Write("{\"Command\":\"Objects\"}");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal string SendFile(string FillePath)
	{
		string address = "http://fs.tax.gov.ua:8609/fs/cmd";
		string result;
		try
		{
			using WebClient webClient = new WebClient();
			webClient.Headers.Add("Content-Type", "application/octet-stream");
			Array array = File.ReadAllBytes(FillePath);
			Array array2 = webClient.UploadData(address, "POST", (byte[])array);
			string @string = Encoding.UTF8.GetString((byte[])array2);
			if (Operators.CompareString(@string.Trim(), "", TextCompare: false) == 0)
			{
				ShowInfaError("Виникла помилка завантаження даних");
			}
			result = @string;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ShowInfaError(ex2.Message);
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal string ControlTin(string KeyOpS, string PasOpS)
	{
		TypErrStrCert typErrStrCert = All.SF.Cert(KeyOpS, PasOpS);
		if (typErrStrCert.errCode > 0)
		{
			return "";
		}
		return typErrStrCert.ReturnTIN.Trim();
	}

	private void ShowInfaError(string TextError)
	{
		All.LgAll.SaveTextToLogAll("ERROR offline accounting", TextError);
	}

	internal string NameFileTIN(string eTin)
	{
		checked
		{
			string result;
			try
			{
				string text = GetDriveSerialNumber().Replace("-", "").Replace(" ", "");
				string text2 = text.Substring(0, 3);
				string text3 = text.Substring(3, 2);
				string text4 = Math.Abs(Conversions.ToInteger(text2) - Conversions.ToUInteger(text3)).ToString();
				int num = 0;
				int num2 = eTin.Length - 1;
				for (int i = 0; i <= num2; i++)
				{
					num += Conversions.ToInteger(eTin[i].ToString());
				}
				result = num + text4 + text2 + eTin + text3;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = "err" + eTin;
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private string GetDriveSerialNumber()
	{
		object objectValue = RuntimeHelpers.GetObjectValue(Interaction.CreateObject("Scripting.FileSystemObject"));
		object instance = objectValue;
		object[] array = new object[1];
		object instance2 = objectValue;
		object[] array2 = new object[1];
		object obj = (array2[0] = AppDomain.CurrentDomain.BaseDirectory);
		array[0] = NewLateBinding.LateGet(instance2, null, "GetDriveName", array2, null, null, null);
		object[] array3 = array;
		bool[] obj2 = new bool[1] { true };
		bool[] array4 = obj2;
		object obj3 = NewLateBinding.LateGet(instance, null, "GetDrive", array, null, null, obj2);
		if (array4[0])
		{
			NewLateBinding.LateSetComplex(instance2, null, "GetDriveName", new object[2]
			{
				obj,
				array3[0]
			}, null, null, OptimisticSet: true, RValueBase: false);
		}
		object objectValue2 = RuntimeHelpers.GetObjectValue(obj3);
		long num = ((!Conversions.ToBoolean(NewLateBinding.LateGet(objectValue2, null, "IsReady", new object[0], null, null, null))) ? (-1) : Conversions.ToLong(NewLateBinding.LateGet(objectValue2, null, "SerialNumber", new object[0], null, null, null)));
		objectValue2 = null;
		objectValue = null;
		return Conversions.ToString(num);
	}

	internal string TextToNumber(string eT)
	{
		string text = "";
		checked
		{
			int num = eT.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(eT[i].ToString(), " ", TextCompare: false) != 0)
				{
					text += eT[i];
					continue;
				}
				return text;
			}
			return "";
		}
	}
}
