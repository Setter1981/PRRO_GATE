using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormCertificate : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("StartTest")]
	private Button _StartTest;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("SelSwrver")]
	private Button _SelSwrver;

	[CompilerGenerated]
	[AccessedThroughProperty("CopyB")]
	private Button _CopyB;

	[CompilerGenerated]
	[AccessedThroughProperty("DelSert")]
	private Button _DelSert;

	internal virtual Button StartTest
	{
		[CompilerGenerated]
		get
		{
			return _StartTest;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = StartTest_Click;
			Button startTest = _StartTest;
			if (startTest != null)
			{
				((Control)startTest).Click -= eventHandler;
			}
			_StartTest = value;
			startTest = _StartTest;
			if (startTest != null)
			{
				((Control)startTest).Click += eventHandler;
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

	[field: AccessedThroughProperty("Label21")]
	internal virtual Label Label21
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("CertificateT")]
	internal virtual TextBox CertificateT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button CopyB
	{
		[CompilerGenerated]
		get
		{
			return _CopyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CopyB_Click;
			Button copyB = _CopyB;
			if (copyB != null)
			{
				((Control)copyB).Click -= eventHandler;
			}
			_CopyB = value;
			copyB = _CopyB;
			if (copyB != null)
			{
				((Control)copyB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button DelSert
	{
		[CompilerGenerated]
		get
		{
			return _DelSert;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = DelSert_Click;
			Button delSert = _DelSert;
			if (delSert != null)
			{
				((Control)delSert).Click -= eventHandler;
			}
			_DelSert = value;
			delSert = _DelSert;
			if (delSert != null)
			{
				((Control)delSert).Click += eventHandler;
			}
		}
	}

	public FormCertificate()
	{
		((Form)this).Load += FormCertificate_Load;
		InitializeComponent();
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
		//IL_00bc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c6: Expected O, but got Unknown
		//IL_01b3: Unknown result type (might be due to invalid IL or missing references)
		//IL_01bd: Expected O, but got Unknown
		//IL_0238: Unknown result type (might be due to invalid IL or missing references)
		//IL_0242: Expected O, but got Unknown
		//IL_02ae: Unknown result type (might be due to invalid IL or missing references)
		//IL_02b8: Expected O, but got Unknown
		//IL_033f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0349: Expected O, but got Unknown
		//IL_03c3: Unknown result type (might be due to invalid IL or missing references)
		//IL_03cd: Expected O, but got Unknown
		//IL_043b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0445: Expected O, but got Unknown
		//IL_04cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_04d6: Expected O, but got Unknown
		//IL_054e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0558: Expected O, but got Unknown
		//IL_05e1: Unknown result type (might be due to invalid IL or missing references)
		//IL_05eb: Expected O, but got Unknown
		//IL_0678: Unknown result type (might be due to invalid IL or missing references)
		//IL_0682: Expected O, but got Unknown
		//IL_06f4: Unknown result type (might be due to invalid IL or missing references)
		//IL_06fe: Expected O, but got Unknown
		//IL_0889: Unknown result type (might be due to invalid IL or missing references)
		//IL_0893: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormCertificate));
		StartTest = new Button();
		KeyB = new Button();
		Label11 = new Label();
		Label10 = new Label();
		PasOpT = new TextBox();
		KeyOpT = new TextBox();
		Server = new TextBox();
		SelSwrver = new Button();
		Label21 = new Label();
		CertificateT = new TextBox();
		CopyB = new Button();
		Label1 = new Label();
		DelSert = new Button();
		((Control)this).SuspendLayout();
		((Control)StartTest).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)StartTest).Location = new Point(22, 145);
		((Control)StartTest).Name = "StartTest";
		((Control)StartTest).Size = new Size(329, 48);
		((Control)StartTest).TabIndex = 1;
		((ButtonBase)StartTest).Text = "Перевірити";
		((ButtonBase)StartTest).UseVisualStyleBackColor = true;
		((Control)KeyB).Location = new Point(596, 17);
		((Control)KeyB).Name = "KeyB";
		((Control)KeyB).Size = new Size(53, 30);
		((Control)KeyB).TabIndex = 25;
		((ButtonBase)KeyB).Text = "...";
		((ButtonBase)KeyB).UseVisualStyleBackColor = true;
		Label11.AutoSize = true;
		((Control)Label11).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label11).Location = new Point(17, 60);
		((Control)Label11).Name = "Label11";
		((Control)Label11).Size = new Size(202, 25);
		((Control)Label11).TabIndex = 29;
		Label11.Text = "Пароль ключа ЕЦП *";
		Label10.AutoSize = true;
		((Control)Label10).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label10).Location = new Point(17, 20);
		((Control)Label10).Name = "Label10";
		((Control)Label10).Size = new Size(121, 25);
		((Control)Label10).TabIndex = 28;
		Label10.Text = "Ключ ЕЦП *";
		((Control)PasOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PasOpT).Location = new Point(258, 57);
		((Control)PasOpT).Name = "PasOpT";
		PasOpT.PasswordChar = '*';
		((Control)PasOpT).Size = new Size(329, 30);
		((Control)PasOpT).TabIndex = 27;
		PasOpT.TextAlign = (HorizontalAlignment)2;
		((Control)KeyOpT).Enabled = false;
		((Control)KeyOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)KeyOpT).Location = new Point(258, 17);
		((Control)KeyOpT).Name = "KeyOpT";
		((Control)KeyOpT).Size = new Size(329, 30);
		((Control)KeyOpT).TabIndex = 26;
		KeyOpT.TextAlign = (HorizontalAlignment)2;
		((Control)Server).Enabled = false;
		((Control)Server).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Server).Location = new Point(258, 100);
		((Control)Server).Name = "Server";
		((Control)Server).Size = new Size(328, 30);
		((Control)Server).TabIndex = 33;
		Server.TextAlign = (HorizontalAlignment)2;
		((Control)SelSwrver).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SelSwrver).Location = new Point(596, 100);
		((Control)SelSwrver).Name = "SelSwrver";
		((Control)SelSwrver).Size = new Size(53, 30);
		((Control)SelSwrver).TabIndex = 32;
		((ButtonBase)SelSwrver).Text = "...";
		((ButtonBase)SelSwrver).UseVisualStyleBackColor = true;
		Label21.AutoSize = true;
		((Control)Label21).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label21).Location = new Point(22, 103);
		((Control)Label21).Name = "Label21";
		((Control)Label21).Size = new Size(77, 25);
		((Control)Label21).TabIndex = 31;
		Label21.Text = "АЦСК *";
		((Control)CertificateT).Enabled = false;
		((Control)CertificateT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CertificateT).Location = new Point(22, 220);
		CertificateT.Multiline = true;
		((Control)CertificateT).Name = "CertificateT";
		((TextBoxBase)CertificateT).ReadOnly = true;
		((Control)CertificateT).Size = new Size(627, 131);
		((Control)CertificateT).TabIndex = 34;
		CertificateT.TextAlign = (HorizontalAlignment)2;
		((Control)CopyB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CopyB).Location = new Point(211, 361);
		((Control)CopyB).Name = "CopyB";
		((Control)CopyB).Size = new Size(438, 30);
		((Control)CopyB).TabIndex = 35;
		((ButtonBase)CopyB).Text = "Скопіювати iдентифікатор ключа суб’єкта";
		((ButtonBase)CopyB).UseVisualStyleBackColor = true;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 9f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(19, 199);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(217, 18);
		((Control)Label1).TabIndex = 36;
		Label1.Text = "Ідентифікатор ключа суб’єкта";
		((Control)DelSert).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)DelSert).Location = new Point(375, 145);
		((Control)DelSert).Name = "DelSert";
		((Control)DelSert).Size = new Size(274, 48);
		((Control)DelSert).TabIndex = 38;
		((ButtonBase)DelSert).Text = "Видалити сертифікат";
		((ButtonBase)DelSert).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(667, 405);
		((Control)this).Controls.Add((Control)(object)DelSert);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)CopyB);
		((Control)this).Controls.Add((Control)(object)CertificateT);
		((Control)this).Controls.Add((Control)(object)Server);
		((Control)this).Controls.Add((Control)(object)SelSwrver);
		((Control)this).Controls.Add((Control)(object)Label21);
		((Control)this).Controls.Add((Control)(object)KeyB);
		((Control)this).Controls.Add((Control)(object)Label11);
		((Control)this).Controls.Add((Control)(object)Label10);
		((Control)this).Controls.Add((Control)(object)PasOpT);
		((Control)this).Controls.Add((Control)(object)KeyOpT);
		((Control)this).Controls.Add((Control)(object)StartTest);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormCertificate";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Сертифікат";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormCertificate_Load(object sender, EventArgs e)
	{
	}

	private void StartTest_Click(object sender, EventArgs e)
	{
		//IL_0089: Unknown result type (might be due to invalid IL or missing references)
		All.RetriesPrt = 1;
		All.SF.SignatureStart();
		All.SF.ErrorShow(ShowWindows: true);
		TypErrStrCert typErrStrCert = All.SF.Cert(KeyOpT.Text.Trim(), PasOpT.Text.Trim());
		if (typErrStrCert.errCode == 0)
		{
			CertificateT.Text = typErrStrCert.ReturnStr;
			Clipboard.SetText(CertificateT.Text);
			if (!All.CertificateTrue(typErrStrCert.ReturnSerial))
			{
				Interaction.MsgBox((object)"Ваш сертифікат відкликано!", (MsgBoxStyle)16, (object)"Увага!");
			}
		}
		else
		{
			CertificateT.Text = "ПОМИЛКА";
			Clipboard.SetText(CertificateT.Text);
		}
		All.SF.SignatureStop();
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
			}
		}
		else
		{
			All.A.AcskSettingsTemp = 2;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			All.A.AcskSettings = All.A.AcskSettingsTemp;
		}
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

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		((Form)new FormServerSelection(NewBase: true)).ShowDialog();
		Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
		All.A.AcskSettings = All.A.AcskSettingsTemp;
	}

	private void CopyB_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(CertificateT.Text);
	}

	private void DelSert_Click(object sender, EventArgs e)
	{
		//IL_0086: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Unknown result type (might be due to invalid IL or missing references)
		((Control)DelSert).Enabled = false;
		All.RetriesPrt = 1;
		All.SF.SignatureStart();
		All.SF.ErrorShow(ShowWindows: true);
		TypErr typErr = All.SF.CertDel(KeyOpT.Text.Trim(), PasOpT.Text.Trim());
		if (typErr.errCode > 0)
		{
			Interaction.MsgBox((object)("Помилка видалення сертифіката: " + typErr.errStr), (MsgBoxStyle)16, (object)"Увага!");
		}
		else
		{
			Interaction.MsgBox((object)"Сертифікат видалено!", (MsgBoxStyle)0, (object)"Видалення сертифікату");
		}
		All.SF.SignatureStop();
		((Control)DelSert).Enabled = true;
	}
}
