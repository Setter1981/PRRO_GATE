using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
public class FormAddTin : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("SelSwrver")]
	private Button _SelSwrver;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("ImportDat")]
	private Button _ImportDat;

	[CompilerGenerated]
	[AccessedThroughProperty("ButtonOk")]
	private Button _ButtonOk;

	[CompilerGenerated]
	[AccessedThroughProperty("ButtonCancel")]
	private Button _ButtonCancel;

	private AccountantОffice AO;

	[field: AccessedThroughProperty("Server")]
	internal virtual TextBox Server
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SelSwrver
	{
		[CompilerGenerated]
		get
		{
			return _SelSwrver;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				((Control)selSwrver).Click -= eventHandler;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				((Control)selSwrver).Click += eventHandler;
			}
		}
	}

	internal virtual Button KeyB
	{
		[CompilerGenerated]
		get
		{
			return _KeyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				((Control)keyB).Click -= eventHandler;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				((Control)keyB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label11")]
	internal virtual Label Label11
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PasOpT")]
	internal virtual TextBox PasOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KeyOpT")]
	internal virtual TextBox KeyOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label21")]
	internal virtual Label Label21
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ImportDat
	{
		[CompilerGenerated]
		get
		{
			return _ImportDat;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ImportDat_Click;
			Button importDat = _ImportDat;
			if (importDat != null)
			{
				((Control)importDat).Click -= eventHandler;
			}
			_ImportDat = value;
			importDat = _ImportDat;
			if (importDat != null)
			{
				((Control)importDat).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxTin")]
	internal virtual TextBox TextBoxTin
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ButtonOk
	{
		[CompilerGenerated]
		get
		{
			return _ButtonOk;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ButtonOk_Click;
			Button buttonOk = _ButtonOk;
			if (buttonOk != null)
			{
				((Control)buttonOk).Click -= eventHandler;
			}
			_ButtonOk = value;
			buttonOk = _ButtonOk;
			if (buttonOk != null)
			{
				((Control)buttonOk).Click += eventHandler;
			}
		}
	}

	internal virtual Button ButtonCancel
	{
		[CompilerGenerated]
		get
		{
			return _ButtonCancel;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ButtonCancel_Click;
			Button buttonCancel = _ButtonCancel;
			if (buttonCancel != null)
			{
				((Control)buttonCancel).Click -= eventHandler;
			}
			_ButtonCancel = value;
			buttonCancel = _ButtonCancel;
			if (buttonCancel != null)
			{
				((Control)buttonCancel).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00c8: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d2: Expected O, but got Unknown
		//IL_014c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0156: Expected O, but got Unknown
		//IL_0241: Unknown result type (might be due to invalid IL or missing references)
		//IL_024b: Expected O, but got Unknown
		//IL_02c6: Unknown result type (might be due to invalid IL or missing references)
		//IL_02d0: Expected O, but got Unknown
		//IL_033c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0346: Expected O, but got Unknown
		//IL_03cd: Unknown result type (might be due to invalid IL or missing references)
		//IL_03d7: Expected O, but got Unknown
		//IL_045d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0467: Expected O, but got Unknown
		//IL_04d3: Unknown result type (might be due to invalid IL or missing references)
		//IL_04dd: Expected O, but got Unknown
		//IL_0577: Unknown result type (might be due to invalid IL or missing references)
		//IL_0581: Expected O, but got Unknown
		//IL_0626: Unknown result type (might be due to invalid IL or missing references)
		//IL_0630: Expected O, but got Unknown
		//IL_06b1: Unknown result type (might be due to invalid IL or missing references)
		//IL_06bb: Expected O, but got Unknown
		//IL_0745: Unknown result type (might be due to invalid IL or missing references)
		//IL_074f: Expected O, but got Unknown
		//IL_08cb: Unknown result type (might be due to invalid IL or missing references)
		//IL_08d5: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormAddTin));
		Server = new TextBox();
		SelSwrver = new Button();
		KeyB = new Button();
		Label11 = new Label();
		Label10 = new Label();
		PasOpT = new TextBox();
		KeyOpT = new TextBox();
		Label21 = new Label();
		ImportDat = new Button();
		TextBoxTin = new TextBox();
		ButtonOk = new Button();
		ButtonCancel = new Button();
		Label1 = new Label();
		((Control)this).SuspendLayout();
		((Control)Server).Enabled = false;
		((Control)Server).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Server).Location = new Point(300, 122);
		((Control)Server).Name = "Server";
		((Control)Server).Size = new Size(402, 30);
		((Control)Server).TabIndex = 32;
		((Control)Server).TabStop = false;
		Server.TextAlign = (HorizontalAlignment)2;
		((Control)SelSwrver).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SelSwrver).Location = new Point(721, 122);
		((Control)SelSwrver).Name = "SelSwrver";
		((Control)SelSwrver).Size = new Size(53, 30);
		((Control)SelSwrver).TabIndex = 31;
		((ButtonBase)SelSwrver).Text = "...";
		((ButtonBase)SelSwrver).UseVisualStyleBackColor = true;
		((Control)KeyB).Location = new Point(721, 28);
		((Control)KeyB).Name = "KeyB";
		((Control)KeyB).Size = new Size(53, 30);
		((Control)KeyB).TabIndex = 25;
		((ButtonBase)KeyB).Text = "...";
		((ButtonBase)KeyB).UseVisualStyleBackColor = true;
		Label11.AutoSize = true;
		((Control)Label11).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label11).Location = new Point(22, 79);
		((Control)Label11).Name = "Label11";
		((Control)Label11).Size = new Size(202, 25);
		((Control)Label11).TabIndex = 30;
		Label11.Text = "Пароль ключа ЕЦП *";
		Label10.AutoSize = true;
		((Control)Label10).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label10).Location = new Point(22, 33);
		((Control)Label10).Name = "Label10";
		((Control)Label10).Size = new Size(121, 25);
		((Control)Label10).TabIndex = 29;
		Label10.Text = "Ключ ЕЦП *";
		((Control)PasOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PasOpT).Location = new Point(300, 76);
		((Control)PasOpT).Name = "PasOpT";
		PasOpT.PasswordChar = '*';
		((Control)PasOpT).Size = new Size(402, 30);
		((Control)PasOpT).TabIndex = 28;
		PasOpT.TextAlign = (HorizontalAlignment)2;
		((Control)KeyOpT).Enabled = false;
		((Control)KeyOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)KeyOpT).Location = new Point(300, 30);
		((Control)KeyOpT).Name = "KeyOpT";
		((Control)KeyOpT).Size = new Size(402, 30);
		((Control)KeyOpT).TabIndex = 27;
		((Control)KeyOpT).TabStop = false;
		KeyOpT.TextAlign = (HorizontalAlignment)2;
		Label21.AutoSize = true;
		((Control)Label21).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label21).Location = new Point(22, 125);
		((Control)Label21).Name = "Label21";
		((Control)Label21).Size = new Size(77, 25);
		((Control)Label21).TabIndex = 26;
		Label21.Text = "АЦСК *";
		((Control)ImportDat).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ImportDat).Location = new Point(27, 175);
		((Control)ImportDat).Name = "ImportDat";
		((Control)ImportDat).Size = new Size(747, 48);
		((Control)ImportDat).TabIndex = 34;
		((ButtonBase)ImportDat).Text = "Завантаження даних з кабінету податкової...";
		((ButtonBase)ImportDat).UseVisualStyleBackColor = true;
		((TextBoxBase)TextBoxTin).BackColor = SystemColors.Window;
		((Control)TextBoxTin).Enabled = false;
		((Control)TextBoxTin).Font = new Font("Microsoft Sans Serif", 16.2f, (FontStyle)1, (GraphicsUnit)3, (byte)204);
		((TextBoxBase)TextBoxTin).ForeColor = SystemColors.WindowText;
		((Control)TextBoxTin).Location = new Point(27, 267);
		TextBoxTin.Multiline = true;
		((Control)TextBoxTin).Name = "TextBoxTin";
		((TextBoxBase)TextBoxTin).ReadOnly = true;
		((Control)TextBoxTin).Size = new Size(747, 180);
		((Control)TextBoxTin).TabIndex = 35;
		((Control)TextBoxTin).TabStop = false;
		TextBoxTin.TextAlign = (HorizontalAlignment)2;
		((Control)ButtonOk).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ButtonOk).Location = new Point(419, 472);
		((Control)ButtonOk).Name = "ButtonOk";
		((Control)ButtonOk).Size = new Size(355, 48);
		((Control)ButtonOk).TabIndex = 36;
		((ButtonBase)ButtonOk).Text = "Підключити";
		((ButtonBase)ButtonOk).UseVisualStyleBackColor = true;
		((Control)ButtonCancel).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ButtonCancel).Location = new Point(27, 472);
		((Control)ButtonCancel).Name = "ButtonCancel";
		((Control)ButtonCancel).Size = new Size(355, 48);
		((Control)ButtonCancel).TabIndex = 37;
		((ButtonBase)ButtonCancel).Text = "Відміна";
		((ButtonBase)ButtonCancel).UseVisualStyleBackColor = true;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(22, 239);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(316, 25);
		((Control)Label1).TabIndex = 38;
		Label1.Text = "Результат завантаження даних:";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(804, 547);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)ButtonCancel);
		((Control)this).Controls.Add((Control)(object)ButtonOk);
		((Control)this).Controls.Add((Control)(object)TextBoxTin);
		((Control)this).Controls.Add((Control)(object)ImportDat);
		((Control)this).Controls.Add((Control)(object)Server);
		((Control)this).Controls.Add((Control)(object)SelSwrver);
		((Control)this).Controls.Add((Control)(object)KeyB);
		((Control)this).Controls.Add((Control)(object)Label11);
		((Control)this).Controls.Add((Control)(object)Label10);
		((Control)this).Controls.Add((Control)(object)PasOpT);
		((Control)this).Controls.Add((Control)(object)KeyOpT);
		((Control)this).Controls.Add((Control)(object)Label21);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormAddTin";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Завантаження даних про підприємство";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormAddTin(bool e = true)
	{
		((Form)this).Load += FormAddTin_Load;
		AO = new AccountantОffice();
		InitializeComponent();
		if (!e)
		{
			((ButtonBase)ButtonOk).Text = "Оновити дані";
			((Form)this).Text = "Завантаження даних з кабінету податкової для оновлення...";
		}
	}

	private void FormAddTin_Load(object sender, EventArgs e)
	{
		All.TinBux = "";
		((Control)ButtonOk).Enabled = false;
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string text = PathKey();
		if (Operators.CompareString(text, "", false) == 0)
		{
			return;
		}
		KeyOpT.Text = text;
		string text2 = KeyTip(text);
		if (Operators.CompareString(text2, "zs2", false) != 0)
		{
			if (Operators.CompareString(text2, "jks", false) == 0)
			{
				All.A.AcskSettingsTemp = 4;
				Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
				All.A.AcskSettings = All.A.AcskSettingsTemp;
				Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
			}
		}
		else
		{
			All.A.AcskSettingsTemp = 2;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			All.A.AcskSettings = All.A.AcskSettingsTemp;
			Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
		}
		((Control)PasOpT).Focus();
	}

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		//IL_0007: Unknown result type (might be due to invalid IL or missing references)
		FormServerSelection formServerSelection = new FormServerSelection(NewBase: true);
		((Form)formServerSelection).ShowDialog();
		((Component)(object)formServerSelection).Dispose();
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
	}

	private void ImportDat_Click(object sender, EventArgs e)
	{
		//IL_0056: Unknown result type (might be due to invalid IL or missing references)
		All.SF.SignatureStart();
		if ((Operators.CompareString(KeyOpT.Text.Trim(), "", false) == 0) | (Operators.CompareString(PasOpT.Text.Trim(), "", false) == 0))
		{
			Interaction.MsgBox((object)"Обов'язкові поля для завантаження даних з кабінету податкової:\r\n- Ключ ЕЦП\r\n- Пароль до ключа ЕЦП\r\n- АЦСК", (MsgBoxStyle)48, (object)"Завантаження даних");
			((Control)ButtonOk).Enabled = false;
			return;
		}
		string text = All.MyDoc() + "\\WebCheck\\Temp\\objects.txt";
		if (File.Exists(text))
		{
			FileSystem.DeleteFile(text);
		}
		if (File.Exists(text + ".p7s"))
		{
			FileSystem.DeleteFile(text + ".p7s");
		}
		if (!AO.FileForSend(text))
		{
			TextBoxTin.Text = "Помилка створення файлу!";
			((Control)ButtonOk).Enabled = false;
			return;
		}
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		All.SF.SetServer();
		All.RetriesPrt = 3;
		All.SF.ErrorShow(ShowWindows: true);
		if (All.SF.SignatureFile(KeyOpT.Text.Trim(), PasOpT.Text.Trim(), text).errCode > 0)
		{
			TextBoxTin.Text = "Помилка підпису!";
			((Control)ButtonOk).Enabled = false;
			return;
		}
		All.SF.ErrorShow(ShowWindows: false);
		All.SF.SetServer();
		string text2 = AO.SendFile(text + ".p7s");
		if (Operators.CompareString(text2.Trim(), "", false) == 0)
		{
			TextBoxTin.Text = "Помилка завантаження даних з кабінета податкової - перевірьте пароль ключа ЕЦП - спробуйте увійти в кабінет податкової з цим ключем ,  перевірити список доступних ПРРО і касирів.";
			((Control)ButtonOk).Enabled = false;
			return;
		}
		All.LgAll.SaveTextToLogAll("ADD TIN", text2);
		if (AO.Dereban(text2))
		{
			string text3 = AO.ControlTin(KeyOpT.Text.Trim(), PasOpT.Text.Trim());
			if (Operators.CompareString(text3, AO.InfaTin.TIN, false) != 0)
			{
				TextBoxTin.Text = "Для завантаження даних використовуйте ЕЦП власника ПРРО.";
				All.LgAll.SaveTextToLogAll("ErrorControlTIN", text3 + " - " + AO.InfaTin.TIN);
				((Control)ButtonOk).Enabled = false;
				return;
			}
			TextBoxTin.Text = "ЕДРОПУ/ІНН :   " + AO.InfaTin.TIN + "   " + AO.InfaTin.OrgName + "   Кількість фіскальних номерів   " + AO.InfaC;
			((Control)ButtonOk).Enabled = true;
		}
		else
		{
			TextBoxTin.Text = "Помилка завантаження даних з кабінета податкової - перевірьте пароль ключа ЕЦП - спробуйте увійти в кабінет податкової з цим ключем ,  перевірити список доступних ПРРО і касирів.";
			((Control)ButtonOk).Enabled = false;
		}
	}

	private void ButtonCancel_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void ButtonOk_Click(object sender, EventArgs e)
	{
		//IL_0419: Unknown result type (might be due to invalid IL or missing references)
		if (AO.InfaC <= 0)
		{
			return;
		}
		All.ArS.AddFn(AO.InfaTin.TIN);
		All.ArS.StringWriteFN(AO.InfaTin.TIN, "TIN", AO.InfaTin.TIN);
		All.ArS.StringWriteFN(AO.InfaTin.TIN, "OrgName", AO.InfaTin.OrgName);
		All.ArS.StringWriteFN(AO.InfaTin.TIN, "Address", AO.InfaTin.Address);
		checked
		{
			int num = AO.InfaC - 1;
			for (int i = 0; i <= num; i++)
			{
				CreatingFolder(AO.Infa[i].NumFiscal);
				try
				{
					string text = All.MyDoc() + "\\WebCheck\\Temp\\objects.txt.p7s";
					string text2 = All.MyDoc() + "\\WebCheck\\Archive\\All\\" + AO.NameFileTIN(AO.InfaTin.TIN) + ".p7s";
					((ServerComputer)MyProject.Computer).FileSystem.CopyFile(text, text2, true);
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					ProjectData.ClearProjectError();
				}
				int num2 = AO.IndexKeysTin(AO.InfaTin.TIN, AO.Infa[i].NumFiscal);
				if (num2 < 0)
				{
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Fn", Conversions.ToInteger(i.ToString())), AO.Infa[i].NumFiscal);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Ad", Conversions.ToInteger(i.ToString())), AO.Infa[i].Address);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Na", Conversions.ToInteger(i.ToString())), AO.Infa[i].Name);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Or", Conversions.ToInteger(i.ToString())), AO.Infa[i].OrgName);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Up", Conversions.ToInteger(i.ToString())), "");
				}
				else
				{
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Ad", Conversions.ToInteger(num2.ToString())), AO.Infa[i].Address);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Na", Conversions.ToInteger(num2.ToString())), AO.Infa[i].Name);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Or", Conversions.ToInteger(num2.ToString())), AO.Infa[i].OrgName);
				}
			}
			Interaction.MsgBox((object)("Вдало підключено " + AO.InfaTin.OrgName), (MsgBoxStyle)0, (object)AO.InfaTin.TIN);
			All.TinBux = AO.InfaTin.TIN + "   -   " + AO.InfaTin.OrgName;
			((Form)this).Close();
		}
	}

	private string JsonTest()
	{
		return "{'TaxObjects':[{'Entity':34562,'TaxObjGuid':'AF5689C1A57E03D8E0530A21420720FC','TaxObjId':32100001,'SingleTax':false,'Name':'ФОП СУХЕНКО ОЛЕНА СТАНІСЛАВІВНА','Address':'УКРАЇНА, М.КИЇВ ПОДІЛЬСЬКИЙ Р-Н, пр. Правди 10 В кв 141','Tin':'30273038877799','Ipn':null,'OrgName':'СУХЕНКО ОЛЕНА СТАНІСЛАВІВНА','ChiefCashier':false,'TransactionsRegistrars':[{'NumFiscal':4000034080,'NumLocal':1,'Name':'ВебЧек','Closed':false}]}],'UID':null,'Timestamp':'2024-02-26T17:47:06.9069165+02:00'}";
	}

	private string PathKey()
	{
		//IL_0000: Unknown result type (might be due to invalid IL or missing references)
		//IL_0006: Expected O, but got Unknown
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		//IL_0018: Invalid comparison between Unknown and I4
		OpenFileDialog val = new OpenFileDialog();
		((FileDialog)val).Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if ((int)((CommonDialog)val).ShowDialog() == 1)
		{
			return ((FileDialog)val).FileName;
		}
		return "";
	}

	private string KeyTip(string FilePath)
	{
		FilePath = FilePath.Trim();
		string text = "";
		checked
		{
			try
			{
				text = Conversions.ToString(FilePath[FilePath.Trim().Length - 3]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 2]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 1]);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				text = "";
				ProjectData.ClearProjectError();
			}
			return text.ToLower();
		}
	}

	private void CreatingFolder(string NameF)
	{
		if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + NameF + "\\"))
		{
			Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Archive\\" + NameF + "\\");
		}
	}
}
